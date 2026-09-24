//! Local SQLite store. Every Joplin item is kept as its full plaintext
//! serialisation (`raw`), so unknown fields round-trip untouched; a few
//! columns are denormalised for fast listing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::item::{self, RawItem, FOLDER_FIELDS, NOTE_FIELDS, RESOURCE_FIELDS, TYPE_FOLDER, TYPE_NOTE, TYPE_RESOURCE};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS items (
    id TEXT PRIMARY KEY,
    type INTEGER NOT NULL,
    parent_id TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    updated_time INTEGER NOT NULL DEFAULT 0,
    user_updated_time INTEGER NOT NULL DEFAULT 0,
    deleted_time INTEGER NOT NULL DEFAULT 0,
    is_conflict INTEGER NOT NULL DEFAULT 0,
    raw TEXT NOT NULL,
    encrypted INTEGER NOT NULL DEFAULT 0,
    dirty INTEGER NOT NULL DEFAULT 0,
    sync_time INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS items_by_parent ON items(type, parent_id, deleted_time);
CREATE INDEX IF NOT EXISTS items_dirty ON items(dirty) WHERE dirty = 1;
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

/// Item types Omanote keeps locally. Revisions, master keys (read from
/// `info.json`) and other internal types are left on the server only.
pub const STORED_TYPES: &[i64] = &[
    item::TYPE_NOTE,
    item::TYPE_FOLDER,
    item::TYPE_RESOURCE,
    item::TYPE_TAG,
    item::TYPE_NOTE_TAG,
];

#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub preview: String,
    pub updated_time: i64,
    pub is_conflict: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: String,
    pub parent_id: String,
    /// Full editable text: first line is the Joplin title, the rest is the body.
    pub text: String,
    pub updated_time: i64,
    pub created_time: i64,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub icon: String,
    pub note_count: i64,
}

/// A note or folder in Joplin's trash.
#[derive(Debug, Clone, Serialize)]
pub struct TrashItem {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub is_folder: bool,
    pub deleted_time: i64,
    pub encrypted: bool,
}

pub struct Store {
    conn: Connection,
    /// Decrypted attachment files, `<id>.<ext>` (next to the database).
    res_dir: PathBuf,
}

pub struct DirtyItem {
    pub raw: RawItem,
    pub sync_time: i64,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let res_dir = path.as_ref().parent().unwrap_or(Path::new(".")).join("resources");
        let conn = Connection::open(path)?;
        // WAL + busy timeout: the app, the CLI and the MCP server can share the file.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn, res_dir })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        let res_dir = std::env::temp_dir().join(format!("omanote-res-{}", item::new_id()));
        Ok(Self { conn, res_dir })
    }

    // -- attachments (Joplin resources) ------------------------------------

    fn blob_path(&self, id: &str, ext: &str) -> PathBuf {
        if ext.is_empty() {
            self.res_dir.join(id)
        } else {
            self.res_dir.join(format!("{id}.{ext}"))
        }
    }

    /// Local file of an attachment, if it has been saved or downloaded.
    pub fn resource_file(&self, id: &str) -> Result<Option<PathBuf>> {
        let Some((it, ..)) = self.raw(id)? else { return Ok(None) };
        if it.item_type() != TYPE_RESOURCE {
            return Ok(None);
        }
        let path = self.blob_path(id, it.get("file_extension"));
        Ok(path.exists().then_some(path))
    }

    /// Where a downloaded attachment is saved (the resource item must be known).
    pub fn save_resource_blob(&self, id: &str, bytes: &[u8]) -> Result<PathBuf> {
        let (it, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("resource {id} not found")))?;
        std::fs::create_dir_all(&self.res_dir).map_err(|e| Error::Sync(e.to_string()))?;
        let path = self.blob_path(id, it.get("file_extension"));
        std::fs::write(&path, bytes).map_err(|e| Error::Sync(e.to_string()))?;
        Ok(path)
    }

    /// Stores a new attachment; returns its id. Insert `![name](:/id)` in a note to show it.
    pub fn add_resource(&self, bytes: &[u8], mime: &str, filename: &str) -> Result<String> {
        let id = item::new_id();
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .or_else(|| mime.strip_prefix("image/").map(|e| e.replace("jpeg", "jpg")))
            .unwrap_or_default();
        let now = item::now_ms();
        let mut it = RawItem::new_with_fields(TYPE_RESOURCE, RESOURCE_FIELDS);
        it.set("id", &id);
        it.title = Some(filename.to_string());
        it.set("mime", mime);
        it.set("filename", filename);
        it.set("file_extension", &ext);
        it.set("size", bytes.len().to_string());
        for k in ["created_time", "updated_time", "user_created_time", "user_updated_time", "blob_updated_time"] {
            if k == "blob_updated_time" {
                it.set(k, now.to_string());
            } else {
                it.set_time(k, now);
            }
        }
        std::fs::create_dir_all(&self.res_dir).map_err(|e| Error::Sync(e.to_string()))?;
        std::fs::write(self.blob_path(&id, &ext), bytes).map_err(|e| Error::Sync(e.to_string()))?;
        self.put_local(&it)?;
        Ok(id)
    }

    /// Forgets the delta position: the next sync re-examines every item on the
    /// server and downloads what is missing or outdated (a repair tool).
    pub fn reset_delta(&self) -> Result<()> {
        self.kv_delete("delta_cursor")
    }

    /// Wipes all local data (used when switching server/account).
    pub fn reset(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM items; DELETE FROM kv;")?;
        Ok(())
    }

    // -- key/value ---------------------------------------------------------

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM kv WHERE key = ?", [key])?;
        Ok(())
    }

    // -- raw item access (used by sync) -----------------------------------

    fn upsert(&self, it: &RawItem, encrypted: bool, dirty: bool, sync_time: Option<i64>) -> Result<()> {
        let is_conflict = it.int("is_conflict");
        self.conn.execute(
            "INSERT INTO items(id, type, parent_id, title, body, updated_time, user_updated_time, deleted_time,
                               is_conflict, raw, encrypted, dirty, sync_time)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, COALESCE(?13, 0))
             ON CONFLICT(id) DO UPDATE SET
                type = excluded.type, parent_id = excluded.parent_id, title = excluded.title,
                body = excluded.body, updated_time = excluded.updated_time,
                user_updated_time = excluded.user_updated_time, deleted_time = excluded.deleted_time,
                is_conflict = excluded.is_conflict, raw = excluded.raw, encrypted = excluded.encrypted,
                dirty = excluded.dirty, sync_time = COALESCE(?13, items.sync_time)",
            params![
                it.id(),
                it.item_type(),
                it.get("parent_id"),
                it.title.as_deref().unwrap_or(""),
                it.body.as_deref().unwrap_or(""),
                it.time("updated_time"),
                it.time("user_updated_time"),
                it.int("deleted_time"),
                is_conflict,
                it.serialize(),
                encrypted as i64,
                dirty as i64,
                sync_time,
            ],
        )?;
        Ok(())
    }

    /// Stores an item received from the server (clean, in sync).
    pub fn put_remote(&self, it: &RawItem, encrypted: bool) -> Result<()> {
        self.upsert(it, encrypted, false, Some(it.time("updated_time")))
    }

    /// Stores a locally modified item, to be uploaded on next sync.
    pub fn put_local(&self, it: &RawItem) -> Result<()> {
        self.upsert(it, false, true, None)
    }

    pub fn raw(&self, id: &str) -> Result<Option<(RawItem, bool, bool, i64)>> {
        let row: Option<(String, i64, i64, i64)> = self
            .conn
            .query_row(
                "SELECT raw, encrypted, dirty, sync_time FROM items WHERE id = ?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((raw, enc, dirty, st)) => Ok(Some((RawItem::parse(&raw)?, enc != 0, dirty != 0, st))),
        }
    }

    /// Local `updated_time` of an item, if present.
    pub fn updated_time(&self, id: &str) -> Result<Option<(i64, bool)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT updated_time, dirty FROM items WHERE id = ?",
                [id],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .optional()?)
    }

    pub fn dirty_items(&self) -> Result<Vec<DirtyItem>> {
        let mut st = self
            .conn
            .prepare("SELECT raw, sync_time FROM items WHERE dirty = 1 ORDER BY CASE type WHEN 2 THEN 0 ELSE 1 END, updated_time")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (raw, sync_time) = row?;
            out.push(DirtyItem { raw: RawItem::parse(&raw)?, sync_time });
        }
        Ok(out)
    }

    /// Marks every readable item for upload, as if edited now but without changing it:
    /// `sync_time = updated_time`, so only a server copy that is really newer counts
    /// as a conflict. Items already edited here keep their real `sync_time`: the
    /// download skips remote versions older than a local edit, so only that value
    /// still reveals a change made elsewhere since the last sync (→ conflict copy).
    /// Used by the re-upload of local data.
    pub fn mark_all_for_reupload(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE items SET dirty = 1, sync_time = updated_time
             WHERE dirty = 0 AND encrypted = 0 AND type IN (1, 2, 4, 5, 6)",
            [],
        )?)
    }

    /// Marks an uploaded item as clean, unless it was edited again meanwhile.
    pub fn mark_synced(&self, id: &str, uploaded_updated_time: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE items SET sync_time = ?2, dirty = CASE WHEN updated_time > ?2 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id, uploaded_updated_time],
        )?;
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM items WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn encrypted_items(&self) -> Result<Vec<RawItem>> {
        let mut st = self.conn.prepare("SELECT raw FROM items WHERE encrypted = 1")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(RawItem::parse(&r?)?)).collect()
    }

    pub fn pending_deletions(&self) -> Result<Vec<String>> {
        Ok(self
            .kv_get("pending_deletions")?
            .map(|s| serde_json::from_str(&s).unwrap_or_default())
            .unwrap_or_default())
    }

    fn set_pending_deletions(&self, ids: &[String]) -> Result<()> {
        self.kv_set("pending_deletions", &serde_json::to_string(ids)?)
    }

    pub fn clear_pending_deletion(&self, id: &str) -> Result<()> {
        let mut ids = self.pending_deletions()?;
        ids.retain(|x| x != id);
        self.set_pending_deletions(&ids)
    }

    // -- folders -----------------------------------------------------------

    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut st = self.conn.prepare(
            "SELECT f.id, f.parent_id, f.title, f.raw,
                    (SELECT COUNT(*) FROM items n WHERE n.type = 1 AND n.parent_id = f.id AND n.deleted_time = 0)
             FROM items f WHERE f.type = 2 AND f.deleted_time = 0 ORDER BY f.title COLLATE NOCASE",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?, r.get(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, parent_id, title, raw, note_count) = row?;
            let icon = RawItem::parse(&raw).map(|r| folder_emoji(r.get("icon"))).unwrap_or_default();
            out.push(Folder { id, parent_id, title, icon, note_count });
        }
        Ok(out)
    }

    /// Returns `root` and all its descendant folder ids.
    pub fn folder_subtree(&self, root: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(
            "WITH RECURSIVE t(id) AS (SELECT ?1 UNION SELECT i.id FROM items i JOIN t ON i.parent_id = t.id
                                     WHERE i.type = 2 AND i.deleted_time = 0)
             SELECT id FROM t",
        )?;
        let rows = st.query_map([root], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn create_folder(&self, title: &str, parent_id: &str) -> Result<Folder> {
        let now = item::now_ms();
        let mut it = RawItem::new_with_fields(TYPE_FOLDER, FOLDER_FIELDS);
        let id = item::new_id();
        it.set("id", &id);
        it.set("parent_id", parent_id);
        for k in ["created_time", "updated_time", "user_created_time", "user_updated_time"] {
            it.set_time(k, now);
        }
        it.title = Some(single_line(title));
        self.put_local(&it)?;
        Ok(Folder { id, parent_id: parent_id.into(), title: single_line(title), icon: String::new(), note_count: 0 })
    }

    pub fn update_folder(&self, id: &str, title: Option<&str>, parent_id: Option<&str>) -> Result<()> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("folder {id} not found")))?;
        if enc {
            return Err(Error::Crypto("folder is still encrypted".into()));
        }
        if let Some(t) = title {
            it.title = Some(single_line(t));
        }
        if let Some(p) = parent_id {
            if self.folder_subtree(id)?.iter().any(|f| f == p) {
                return Err(Error::Sync("cannot move a folder inside itself".into()));
            }
            it.set("parent_id", p);
        }
        touch(&mut it);
        self.put_local(&it)
    }

    // -- notes -------------------------------------------------------------

    /// Notes in the given folders (most recently edited first).
    pub fn notes_in(&self, folder_ids: &[String]) -> Result<Vec<NoteSummary>> {
        if folder_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = vec!["?"; folder_ids.len()].join(",");
        let sql = format!(
            "SELECT id, parent_id, title, substr(body, 1, 200), user_updated_time, updated_time, is_conflict, encrypted
             FROM items WHERE type = 1 AND deleted_time = 0 AND parent_id IN ({placeholders})
             ORDER BY MAX(user_updated_time, updated_time) DESC"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(rusqlite::params_from_iter(folder_ids), |r| {
            let uut: i64 = r.get(4)?;
            let ut: i64 = r.get(5)?;
            Ok(NoteSummary {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                title: r.get(2)?,
                preview: r.get(3)?,
                updated_time: uut.max(ut),
                is_conflict: r.get::<_, i64>(6)? != 0,
                encrypted: r.get::<_, i64>(7)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn search_notes(&self, folder_ids: &[String], query: &str) -> Result<Vec<NoteSummary>> {
        let q = query.to_lowercase();
        Ok(self
            .notes_in(folder_ids)?
            .into_iter()
            .filter(|n| {
                self.note(&n.id)
                    .ok()
                    .flatten()
                    .map(|full| full.text.to_lowercase().contains(&q))
                    .unwrap_or(false)
            })
            .collect())
    }

    pub fn note(&self, id: &str) -> Result<Option<Note>> {
        let Some((it, enc, ..)) = self.raw(id)? else { return Ok(None) };
        if it.item_type() != TYPE_NOTE {
            return Ok(None);
        }
        Ok(Some(Note {
            id: it.id().to_string(),
            parent_id: it.get("parent_id").to_string(),
            text: join_text(it.title.as_deref().unwrap_or(""), it.body.as_deref().unwrap_or("")),
            updated_time: it.time("user_updated_time").max(it.time("updated_time")),
            created_time: it.time("user_created_time"),
            encrypted: enc,
        }))
    }

    pub fn create_note(&self, parent_id: &str, text: &str) -> Result<Note> {
        let now = item::now_ms();
        let mut it = RawItem::new_with_fields(TYPE_NOTE, NOTE_FIELDS);
        it.set("id", item::new_id());
        it.set("parent_id", parent_id);
        it.set("source", "omanote");
        it.set("source_application", "app.omanote");
        for k in ["created_time", "updated_time", "user_created_time", "user_updated_time"] {
            it.set_time(k, now);
        }
        it.set("order", now.to_string());
        let (title, body) = split_text(text);
        it.title = Some(title);
        it.body = Some(body);
        self.put_local(&it)?;
        Ok(self.note(it.id())?.expect("just saved"))
    }

    pub fn update_note_text(&self, id: &str, text: &str) -> Result<Note> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("note {id} not found")))?;
        if enc {
            return Err(Error::Crypto("note is still encrypted".into()));
        }
        let (title, body) = split_text(text);
        if it.title.as_deref() == Some(title.as_str()) && it.body.as_deref() == Some(body.as_str()) {
            return Ok(self.note(id)?.expect("exists"));
        }
        it.title = Some(title);
        it.body = Some(body);
        touch(&mut it);
        self.put_local(&it)?;
        Ok(self.note(id)?.expect("exists"))
    }

    pub fn move_note(&self, id: &str, parent_id: &str) -> Result<()> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("note {id} not found")))?;
        if enc {
            return Err(Error::Crypto("note is still encrypted".into()));
        }
        it.set("parent_id", parent_id);
        touch_system(&mut it);
        self.put_local(&it)
    }

    /// Brings a note to the front of the stack ("promote").
    pub fn promote(&self, id: &str) -> Result<()> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("note {id} not found")))?;
        if enc {
            return Err(Error::Crypto("note is still encrypted".into()));
        }
        touch(&mut it);
        self.put_local(&it)
    }

    /// Appends a line of text at the end of a note.
    pub fn append_to_note(&self, id: &str, text: &str) -> Result<Note> {
        let note = self.note(id)?.ok_or_else(|| Error::Sync(format!("note {id} not found")))?;
        let joined = if note.text.is_empty() { text.to_string() } else { format!("{}\n{text}", note.text.trim_end_matches('\n')) };
        self.update_note_text(id, &joined)
    }

    /// Changes whenever *another* connection (the CLI, an AI agent) commits.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    /// Looks a folder up by id or (case-insensitive) title.
    pub fn find_folder(&self, key: &str) -> Result<Option<Folder>> {
        let k = key.to_lowercase();
        Ok(self.folders()?.into_iter().find(|f| f.id == key || f.title.to_lowercase() == k))
    }

    /// Moves an item to Joplin's trash (sets `deleted_time`, like Joplin ≥ 3.0).
    pub fn trash(&self, id: &str) -> Result<()> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("item {id} not found")))?;
        if enc {
            return Err(Error::Crypto("item is still encrypted".into()));
        }
        it.set("deleted_time", item::now_ms().to_string());
        touch_system(&mut it);
        self.put_local(&it)
    }

    /// Permanently deletes an item locally and on the server at next sync.
    pub fn delete_permanently(&self, id: &str) -> Result<()> {
        let mut ids = self.pending_deletions()?;
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
        self.set_pending_deletions(&ids)?;
        self.remove(id)
    }

    // -- trash -------------------------------------------------------------

    /// `(id, parent_id, deleted)` of every folder, trashed ones included.
    fn all_folders(&self) -> Result<Vec<(String, String, bool)>> {
        let mut st = self.conn.prepare("SELECT id, parent_id, deleted_time > 0 FROM items WHERE type = 2")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Items (notes and folders, trashed or not) below folder `id`, deepest last.
    fn descendants(&self, id: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare(
            "WITH RECURSIVE t(id, type) AS (SELECT id, type FROM items WHERE parent_id = ?1 AND type IN (1, 2)
                 UNION ALL SELECT i.id, i.type FROM items i JOIN t ON i.parent_id = t.id WHERE t.type = 2 AND i.type IN (1, 2))
             SELECT id FROM t",
        )?;
        let rows = st.query_map([id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Trashed notes and folders whose parent chain reaches `root` ("" = all of Joplin),
    /// most recently deleted first. `parent_id` of an item inside a trashed folder is
    /// that folder, so the UI can nest them like Joplin's trash does.
    pub fn trashed(&self, root: &str) -> Result<Vec<TrashItem>> {
        let parents: HashMap<String, String> =
            self.all_folders()?.into_iter().map(|(id, parent, _)| (id, parent)).collect();
        let under_root = |start: &str| {
            let mut id = start.to_string();
            for _ in 0..64 {
                if id == root {
                    return true;
                }
                match parents.get(&id) {
                    Some(p) => id = p.clone(),
                    None => return root.is_empty(),
                }
            }
            false
        };
        let mut st = self.conn.prepare(
            "SELECT id, parent_id, type, title, deleted_time, encrypted FROM items
             WHERE type IN (1, 2) AND deleted_time > 0 ORDER BY deleted_time DESC, title COLLATE NOCASE",
        )?;
        let rows = st.query_map([], |r| {
            Ok(TrashItem {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                is_folder: r.get::<_, i64>(2)? == TYPE_FOLDER,
                title: r.get(3)?,
                deleted_time: r.get(4)?,
                encrypted: r.get::<_, i64>(5)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for it in rows {
            let it = it?;
            if under_root(&it.parent_id) {
                out.push(it);
            }
        }
        Ok(out)
    }

    fn set_deleted(&self, id: &str, deleted: i64, parent: Option<&str>) -> Result<()> {
        let (mut it, enc, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("item {id} not found")))?;
        if enc {
            return Err(Error::Crypto("item is still encrypted".into()));
        }
        it.set("deleted_time", deleted.to_string());
        if let Some(p) = parent {
            it.set("parent_id", p);
        }
        touch_system(&mut it);
        self.put_local(&it)
    }

    /// Takes an item out of the trash, like Joplin: a folder comes back with everything
    /// trashed inside it. When the original parent is gone or still in the trash, a folder
    /// goes to `folder_parent` (the top of the tree the user sees) and a note to
    /// `note_folder`.
    pub fn restore(&self, id: &str, note_folder: &str, folder_parent: &str) -> Result<()> {
        let (it, ..) = self.raw(id)?.ok_or_else(|| Error::Sync(format!("item {id} not found")))?;
        let folders = self.all_folders()?;
        let parent = it.get("parent_id").to_string();
        let parent_alive = folders.iter().any(|(f, _, deleted)| *f == parent && !deleted);
        if it.item_type() == TYPE_FOLDER {
            let keep = parent.is_empty() || parent_alive;
            self.set_deleted(id, 0, (!keep).then_some(folder_parent))?;
            for child in self.descendants(id)? {
                if matches!(self.raw(&child)?, Some((c, false, ..)) if c.get("deleted_time").parse::<i64>().unwrap_or(0) > 0) {
                    self.set_deleted(&child, 0, None)?;
                }
            }
            Ok(())
        } else {
            self.set_deleted(id, 0, (!parent_alive).then_some(note_folder))
        }
    }

    /// Deletes a trashed item for good (a folder with all it contains), locally now and
    /// on the server at the next sync.
    pub fn purge(&self, id: &str) -> Result<()> {
        for child in self.descendants(id)?.iter().rev() {
            self.delete_permanently(child)?;
        }
        self.delete_permanently(id)
    }

    /// Permanently deletes everything in the trash under `root` ("" = all of Joplin).
    pub fn empty_trash(&self, root: &str) -> Result<usize> {
        let items = self.trashed(root)?;
        let mut n = 0;
        for it in &items {
            if self.raw(&it.id)?.is_some() {
                self.purge(&it.id)?;
                n += 1;
            }
        }
        Ok(n)
    }
}

/// Joplin stores a folder icon as JSON: `{"emoji":"🏦","name":"bank","type":1}`
/// (type 1 = emoji; other types are images, not shown by Omanote). Returns the
/// emoji only; the original value is left untouched in the item.
pub fn folder_emoji(icon: &str) -> String {
    let icon = icon.trim();
    if icon.is_empty() {
        return String::new();
    }
    if icon.starts_with('{') {
        return serde_json::from_str::<serde_json::Value>(icon)
            .ok()
            .and_then(|v| v.get("emoji").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_default();
    }
    // Very old clients stored the bare emoji.
    if icon.chars().count() <= 8 { icon.to_string() } else { String::new() }
}

fn touch(it: &mut RawItem) {
    let now = item::now_ms();
    it.set_time("updated_time", now);
    it.set_time("user_updated_time", now);
}

/// System-level change (move, trash): Joplin bumps `updated_time` only.
fn touch_system(it: &mut RawItem) {
    it.set_time("updated_time", item::now_ms());
}

fn single_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_string()
}

/// Omanote edits a note as a single text: line 1 ↔ Joplin title, rest ↔ body.
pub fn split_text(text: &str) -> (String, String) {
    let text = text.replace("\r\n", "\n");
    match text.split_once('\n') {
        Some((t, b)) => (t.trim_end().to_string(), b.to_string()),
        None => (text.trim_end().to_string(), String::new()),
    }
}

pub fn join_text(title: &str, body: &str) -> String {
    if body.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lifecycle() {
        let s = Store::open_in_memory().unwrap();
        let f = s.create_folder("Omanote", "").unwrap();
        let sub = s.create_folder("Groceries", &f.id).unwrap();
        let n = s.create_note(&sub.id, "Shopping\n- milk\n- bread").unwrap();
        assert_eq!(n.text, "Shopping\n- milk\n- bread");
        let (raw, ..) = s.raw(&n.id).unwrap().unwrap();
        assert_eq!(raw.title.as_deref(), Some("Shopping"));
        assert_eq!(raw.body.as_deref(), Some("- milk\n- bread"));

        let tree = s.folder_subtree(&f.id).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(s.notes_in(&tree).unwrap().len(), 1);
        assert_eq!(s.dirty_items().unwrap().len(), 3);

        s.update_note_text(&n.id, "Shopping\n- milk").unwrap();
        assert_eq!(s.search_notes(&tree, "MILK").unwrap().len(), 1);
        assert!(s.update_folder(&f.id, None, Some(&sub.id)).is_err());

        s.trash(&n.id).unwrap();
        assert_eq!(s.notes_in(&tree).unwrap().len(), 0);
    }

    #[test]
    fn trash_restore_purge() {
        let s = Store::open_in_memory().unwrap();
        let home = s.create_folder("Omanote", "").unwrap();
        let work = s.create_folder("Work", "").unwrap();
        let sub = s.create_folder("Projects", &work.id).unwrap();
        let a = s.create_note(&sub.id, "A").unwrap();
        let b = s.create_note(&home.id, "B").unwrap();
        for id in [&a.id, &sub.id, &work.id, &b.id] {
            s.trash(id).unwrap();
        }
        assert_eq!(s.trashed("").unwrap().len(), 4);
        assert_eq!(s.trashed(&home.id).unwrap().len(), 1);

        // A folder comes back with what was trashed inside it.
        s.restore(&work.id, &home.id, "").unwrap();
        assert_eq!(s.notes_in(&s.folder_subtree(&work.id).unwrap()).unwrap().len(), 1);
        assert_eq!(s.trashed("").unwrap().len(), 1);

        // A note whose folder is still trashed goes to the fallback folder.
        s.trash(&sub.id).unwrap();
        s.trash(&a.id).unwrap();
        s.purge(&sub.id).unwrap();
        assert!(s.raw(&a.id).unwrap().is_none());
        assert_eq!(s.pending_deletions().unwrap().len(), 2);
        s.restore(&b.id, &home.id, "").unwrap();
        assert!(s.trashed("").unwrap().is_empty());

        let c = s.create_note(&work.id, "C").unwrap();
        s.trash(&work.id).unwrap();
        s.trash(&c.id).unwrap();
        s.purge(&work.id).unwrap();
        s.restore(&home.id, &home.id, "").unwrap();
        let d = s.create_note(&home.id, "D").unwrap();
        s.trash(&d.id).unwrap();
        assert_eq!(s.empty_trash("").unwrap(), 1);
        assert!(s.raw(&c.id).unwrap().is_none());
    }

    #[test]
    fn restored_folder_stays_in_the_visible_tree() {
        let s = Store::open_in_memory().unwrap();
        let home = s.create_folder("Omanote", "").unwrap();
        let outer = s.create_folder("Projects", &home.id).unwrap();
        let inner = s.create_folder("Old", &outer.id).unwrap();
        s.trash(&inner.id).unwrap();
        s.trash(&outer.id).unwrap();
        s.purge(&outer.id).unwrap_or(()); // outer and inner gone for good…
        let lost = s.create_folder("Orphan", &outer.id).unwrap(); // …a folder whose parent is gone
        s.trash(&lost.id).unwrap();
        s.restore(&lost.id, &home.id, &home.id).unwrap();
        assert_eq!(s.raw(&lost.id).unwrap().unwrap().0.get("parent_id"), home.id);
    }

    #[test]
    fn reupload_keeps_local_edits_detectable() {
        let s = Store::open_in_memory().unwrap();
        let f = s.create_folder("Omanote", "").unwrap();
        let n = s.create_note(&f.id, "A").unwrap();
        s.mark_synced(&f.id, s.raw(&f.id).unwrap().unwrap().0.time("updated_time")).unwrap();
        s.conn.execute("UPDATE items SET sync_time = 1 WHERE id = ?", [&n.id]).unwrap(); // edited since
        s.mark_all_for_reupload().unwrap();
        assert_eq!(s.raw(&n.id).unwrap().unwrap().3, 1, "a local edit keeps its sync_time");
        let (fi, _, dirty, st) = s.raw(&f.id).unwrap().unwrap();
        assert!(dirty && st == fi.time("updated_time"));
    }

    #[test]
    fn folder_icons() {
        assert_eq!(folder_emoji(r#"{"emoji":"🏦","name":"bank","type":1}"#), "🏦");
        assert_eq!(folder_emoji(r#"{"emoji":"","name":"","type":2,"dataUrl":"data:image/png;base64,AAA"}"#), "");
        assert_eq!(folder_emoji("🏠"), "🏠");
        assert_eq!(folder_emoji("not json and far too long"), "");
        assert_eq!(folder_emoji(""), "");
    }

    #[test]
    fn text_split_round_trip() {
        let cases = [("", ""), ("title only", "title only"), ("t\nb", "t\nb"), ("t\n\nb\n", "t\n\nb\n"), ("t\n", "t")];
        for (input, expected) in cases {
            let (a, b) = split_text(input);
            assert_eq!(join_text(&a, &b), expected);
        }
    }
}
