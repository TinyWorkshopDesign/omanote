//! Synchronisation with Joplin Server, following the same protocol as
//! `@joplin/lib/Synchronizer.ts` (sync target version 3):
//!
//! 1. read `info.json` (E2EE state + master keys) — never written by Omanote;
//! 2. take a *sync* lock so Joplin clients running a migration (exclusive
//!    lock) are respected;
//! 3. upload local changes, detecting conflicts through `updated_time`;
//! 4. download remote changes through the delta API;
//! 5. release the lock.
//!
//! Note conflicts are resolved like Joplin does: the remote version wins and
//! the local version is kept as a separate note flagged `is_conflict = 1`
//! (it shows up in Joplin's "Conflicts" folder).

use std::sync::Mutex;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use crate::api::JoplinServer;
use crate::e2ee::{self, KeyRing, MasterKey};
use crate::error::{Error, Result};
use crate::item::{self, RawItem, TYPE_NOTE, TYPE_RESOURCE};
use crate::store::{Store, STORED_TYPES};

pub const SUPPORTED_SYNC_VERSION: i64 = 3;
const DOWNLOAD_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimedValue<T> {
    pub value: T,
    #[serde(rename = "updatedTime", default)]
    pub updated_time: i64,
}

/// Subset of Joplin's `info.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncInfo {
    pub version: i64,
    #[serde(default)]
    pub e2ee: Option<TimedValue<bool>>,
    #[serde(rename = "activeMasterKeyId", default)]
    pub active_master_key_id: Option<TimedValue<String>>,
    #[serde(rename = "masterKeys", default)]
    pub master_keys: Vec<MasterKey>,
    #[serde(rename = "appMinVersion", default)]
    pub app_min_version: Option<String>,
}

impl SyncInfo {
    pub fn e2ee_enabled(&self) -> bool {
        self.e2ee.as_ref().map(|v| v.value).unwrap_or(false)
    }

    pub fn active_key_id(&self) -> Option<&str> {
        self.active_master_key_id
            .as_ref()
            .map(|v| v.value.as_str())
            .filter(|id| !id.is_empty() && self.master_keys.iter().any(|m| m.id == *id))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted_local: usize,
    pub deleted_remote: usize,
    pub conflicts: usize,
    pub still_encrypted: usize,
    /// E2EE is on but the master password is missing or wrong.
    pub needs_password: bool,
    pub errors: Vec<String>,
}

pub struct Synchronizer<'a> {
    pub api: &'a mut JoplinServer,
    pub store: &'a Mutex<Store>,
    pub keys: &'a mut KeyRing,
    pub client_id: String,
    pub client_type: i64,
}

fn is_item_path(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".md")?;
    (id.len() == 32 && !id.contains('/') && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
}

/// Decrypts every master key with `password`; returns how many succeeded.
pub fn unlock_keys(info: &SyncInfo, password: &str, keys: &mut KeyRing) -> usize {
    let mut ok = 0;
    for mk in &info.master_keys {
        if keys.has(&mk.id) {
            ok += 1;
            continue;
        }
        if let Ok(plain) = e2ee::decrypt_master_key(mk, password) {
            keys.insert(&mk.id, plain);
            ok += 1;
        }
    }
    keys.active_id = info.active_key_id().filter(|id| keys.has(id)).map(String::from);
    ok
}

impl<'a> Synchronizer<'a> {
    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub async fn fetch_info(&mut self) -> Result<SyncInfo> {
        let text = self.api.get("info.json").await?.ok_or_else(|| {
            Error::Sync("the sync target is empty: set up sync with a Joplin app first".into())
        })?;
        let info: SyncInfo = serde_json::from_str(&text)?;
        if info.version != SUPPORTED_SYNC_VERSION {
            return Err(Error::Sync(format!(
                "unsupported sync target version {} (expected {SUPPORTED_SYNC_VERSION})",
                info.version
            )));
        }
        self.db().kv_set("sync_info", &text)?;
        Ok(info)
    }

    pub async fn sync(&mut self) -> Result<SyncReport> {
        let info = self.fetch_info().await?;
        let mut report = SyncReport::default();

        if info.e2ee_enabled() {
            // Keep only the active id in sync with info.json; keys stay loaded.
            self.keys.active_id = info.active_key_id().filter(|id| self.keys.has(id)).map(String::from);
            report.needs_password = self.keys.active_id.is_none();
        } else {
            self.keys.active_id = None;
        }

        self.api.acquire_lock(self.client_type, &self.client_id).await?;
        let result = self.run(&info, &mut report).await;
        let _ = self.api.release_lock(self.client_type, &self.client_id).await;
        result?;

        report.still_encrypted = self.decrypt_pending()?;
        Ok(report)
    }

    async fn run(&mut self, info: &SyncInfo, report: &mut SyncReport) -> Result<()> {
        // Uploading needs the key when E2EE is on; downloading still works.
        if !report.needs_password {
            self.upload(info, report).await?;
        }
        self.download(report).await
    }

    fn serialize_for_sync(&self, info: &SyncInfo, it: &RawItem) -> Result<String> {
        if !info.e2ee_enabled() {
            return Ok(it.serialize());
        }
        let cipher = self.keys.encrypt_string(&it.serialize())?;
        Ok(it.encrypted_envelope(cipher).serialize())
    }

    /// Parses and, when possible, decrypts a remote item.
    fn open_remote(&self, content: &str) -> Result<(RawItem, bool)> {
        let envelope = RawItem::parse(content)?;
        if !envelope.is_encrypted() {
            return Ok((envelope, false));
        }
        match self.keys.decrypt_string(envelope.get("encryption_cipher_text")) {
            Ok(plain) => {
                let mut it = RawItem::parse(&plain)?;
                // Like BaseItem.decrypt: the clear-text updated_time is authoritative.
                it.set("updated_time", envelope.get("updated_time"));
                it.set("encryption_cipher_text", "");
                it.set("encryption_applied", "0");
                Ok((it, false))
            }
            Err(Error::MasterKeyNotLoaded(_)) => Ok((envelope, true)),
            Err(e) => Err(e),
        }
    }

    async fn upload(&mut self, info: &SyncInfo, report: &mut SyncReport) -> Result<()> {
        let deletions = self.db().pending_deletions()?;
        for id in deletions {
            self.api.delete(&format!("{id}.md")).await?;
            self.db().clear_pending_deletion(&id)?;
            report.deleted_remote += 1;
        }

        let dirty = self.db().dirty_items()?;
        for d in dirty {
            let id = d.raw.id().to_string();
            let name = format!("{id}.md");
            let remote = match self.api.get(&name).await? {
                Some(content) => match self.open_remote(&content) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        report.errors.push(format!("{name}: {e}"));
                        continue;
                    }
                },
                None => None,
            };

            let conflict = match &remote {
                // Missing remotely: new item, or deleted elsewhere while edited here → recreate it.
                None => false,
                Some((r, _)) => r.time("updated_time") > d.sync_time,
            };

            if conflict {
                let (remote_item, remote_encrypted) = remote.expect("conflict implies remote");
                report.conflicts += 1;
                if d.raw.item_type() == TYPE_NOTE {
                    let copy = self.make_conflict_copy(&d.raw)?;
                    self.upload_one(info, &copy, report).await?;
                }
                self.db().put_remote(&remote_item, remote_encrypted)?;
                continue;
            }

            if d.raw.item_type() == TYPE_RESOURCE && d.sync_time == 0 {
                // New attachment: the file goes up first, like Joplin does.
                let mut it = d.raw.clone();
                self.upload_blob(info, &mut it).await?;
                self.upload_one(info, &it, report).await?;
                continue;
            }
            self.upload_one(info, &d.raw, report).await?;
        }
        Ok(())
    }

    fn make_conflict_copy(&self, local: &RawItem) -> Result<RawItem> {
        let mut copy = local.clone();
        copy.set("id", item::new_id());
        copy.set("is_conflict", "1");
        copy.set("conflict_original_id", local.id());
        copy.set_time("updated_time", item::now_ms());
        self.db().put_local(&copy)?;
        Ok(copy)
    }

    /// Uploads an attachment file to `.resource/<id>` (FileV1-encrypted under E2EE)
    /// and records in the item whether the blob is encrypted.
    async fn upload_blob(&mut self, info: &SyncInfo, it: &mut RawItem) -> Result<()> {
        let Some(path) = self.db().resource_file(it.id())? else {
            return Ok(()); // nothing local (e.g. created elsewhere): keep the remote blob
        };
        let bytes = std::fs::read(&path).map_err(|e| Error::Sync(e.to_string()))?;
        let (body, encrypted) = if info.e2ee_enabled() {
            (self.keys.encrypt_file(&bytes)?.into_bytes(), true)
        } else {
            (bytes, false)
        };
        self.api.put(&format!(".resource/{}", it.id()), body).await?;
        it.set("encryption_blob_encrypted", if encrypted { "1" } else { "0" });
        Ok(())
    }

    /// Downloads (and decrypts) an attachment file, saving it next to the database.
    pub async fn fetch_resource(&mut self, id: &str) -> Result<Option<std::path::PathBuf>> {
        if let Some(p) = self.db().resource_file(id)? {
            return Ok(Some(p));
        }
        let Some((it, encrypted, ..)) = self.db().raw(id)? else { return Ok(None) };
        if encrypted {
            return Err(Error::MasterKeyNotLoaded(id.to_string()));
        }
        let Some(bytes) = self.api.get_bytes(&format!(".resource/{id}")).await? else { return Ok(None) };
        let plain = if it.get("encryption_blob_encrypted") == "1" || e2ee::is_encrypted_text(&String::from_utf8_lossy(&bytes[..bytes.len().min(5)])) {
            self.keys.decrypt_file(&String::from_utf8_lossy(&bytes))?
        } else {
            bytes
        };
        Ok(Some(self.db().save_resource_blob(id, &plain)?))
    }

    async fn upload_one(&mut self, info: &SyncInfo, it: &RawItem, report: &mut SyncReport) -> Result<()> {
        let body = self.serialize_for_sync(info, it)?;
        self.api.put(&format!("{}.md", it.id()), body.into_bytes()).await?;
        self.db().mark_synced(it.id(), it.time("updated_time"))?;
        report.uploaded += 1;
        Ok(())
    }

    async fn download(&mut self, report: &mut SyncReport) -> Result<()> {
        let mut cursor = self.db().kv_get("delta_cursor")?;
        loop {
            let page = self.api.delta(cursor.as_deref()).await?;

            // Collapse the page to the last change of each item.
            let mut latest: indexmap::IndexMap<String, (i64, Option<i64>)> = indexmap::IndexMap::new();
            for ch in &page.items {
                if let Some(id) = is_item_path(&ch.item_name) {
                    latest.shift_remove(id);
                    latest.insert(id.to_string(), (ch.change_type, ch.jop_updated_time));
                }
            }

            let mut to_fetch = Vec::new();
            for (id, (change_type, jop_updated)) in &latest {
                let local = self.db().updated_time(id)?;
                if *change_type == 3 {
                    match local {
                        Some((_, true)) => {} // edited here: it will be re-uploaded
                        Some(_) => {
                            self.db().remove(id)?;
                            report.deleted_local += 1;
                        }
                        None => {}
                    }
                    continue;
                }
                if let (Some((local_updated, _)), Some(j)) = (local, jop_updated) {
                    if local_updated == *j {
                        continue;
                    }
                }
                to_fetch.push(id.clone());
            }

            if !to_fetch.is_empty() {
                // Make sure the session exists once, then fetch concurrently.
                if self.api.session_id().is_none() {
                    self.api.login().await?;
                }
                let api = self.api.clone();
                let fetched: Vec<(String, Result<Option<String>>)> = stream::iter(to_fetch)
                    .map(|id| {
                        let mut api = api.clone();
                        async move {
                            let r = api.get(&format!("{id}.md")).await;
                            (id, r)
                        }
                    })
                    .buffer_unordered(DOWNLOAD_CONCURRENCY)
                    .collect()
                    .await;

                for (id, res) in fetched {
                    let Some(content) = res? else { continue };
                    let (remote, encrypted) = match self.open_remote(&content) {
                        Ok(r) => r,
                        Err(e) => {
                            report.errors.push(format!("{id}.md: {e}"));
                            continue;
                        }
                    };
                    if !STORED_TYPES.contains(&remote.item_type()) {
                        continue;
                    }
                    let local = self.db().raw(&id)?;
                    match local {
                        None => {
                            self.db().put_remote(&remote, encrypted)?;
                            report.downloaded += 1;
                        }
                        Some((local_item, _, dirty, _)) => {
                            if remote.time("updated_time") <= local_item.time("updated_time") {
                                continue;
                            }
                            if dirty && local_item.item_type() == TYPE_NOTE {
                                // Edited here while it changed remotely.
                                self.make_conflict_copy(&local_item)?;
                                report.conflicts += 1;
                            }
                            self.db().put_remote(&remote, encrypted)?;
                            report.downloaded += 1;
                        }
                    }
                }
            }

            cursor = Some(page.cursor.clone());
            self.db().kv_set("delta_cursor", &page.cursor)?;
            if !page.has_more {
                break;
            }
        }
        Ok(())
    }

    /// Decrypts items that were downloaded before the key was available.
    /// Returns how many are still encrypted.
    pub fn decrypt_pending(&self) -> Result<usize> {
        let items = self.db().encrypted_items()?;
        let mut remaining = 0;
        for envelope in items {
            match self.open_remote(&envelope.serialize()) {
                Ok((plain, false)) => self.db().put_remote(&plain, false)?,
                _ => remaining += 1,
            }
        }
        Ok(remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_paths() {
        assert_eq!(is_item_path("0123456789abcdef0123456789abcdef.md"), Some("0123456789abcdef0123456789abcdef"));
        assert_eq!(is_item_path("info.json"), None);
        assert_eq!(is_item_path(".resource/0123456789abcdef0123456789abcdef"), None);
        assert_eq!(is_item_path("locks/1_1_abc.json"), None);
    }

    #[test]
    fn parses_info_json() {
        let info: SyncInfo = serde_json::from_str(r#"{"version":3,"e2ee":{"value":true,"updatedTime":1},"activeMasterKeyId":{"value":"aaaabbbbccccddddeeeeffff00001111","updatedTime":1},"masterKeys":[{"id":"aaaabbbbccccddddeeeeffff00001111","created_time":1,"updated_time":1,"source_application":"x","encryption_method":8,"checksum":"","content":"{}","hasBeenUsed":true}],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}"#).unwrap();
        assert!(info.e2ee_enabled());
        assert_eq!(info.active_key_id(), Some("aaaabbbbccccddddeeeeffff00001111"));
    }
}
