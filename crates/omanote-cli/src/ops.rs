//! Operations shared by the CLI commands and the MCP tools.

use std::path::PathBuf;
use std::sync::Mutex;

use omanote_core::api::{JoplinServer, LOCK_CLIENT_DESKTOP};
use omanote_core::config::{self, Config};
use omanote_core::e2ee::KeyRing;
use omanote_core::secrets;
use omanote_core::store::{Note, NoteSummary, Store};
use omanote_core::sync::{unlock_keys, SyncReport, Synchronizer};
use serde::Serialize;

pub struct Ctx {
    dir: PathBuf,
    cfg: Config,
    store: Mutex<Store>,
}

#[derive(Serialize)]
pub struct FolderOut {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub note_count: i64,
    /// Nesting level below the working notebook.
    pub depth: usize,
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

impl Ctx {
    pub fn open() -> Result<Ctx, String> {
        let dir = config::default_data_dir();
        let db = dir.join(config::DB_FILE);
        if !db.exists() {
            return Err(format!(
                "no Omanote data in {}: open the Omanote app once and connect it to Joplin Server",
                dir.display()
            ));
        }
        let cfg = Config::load(&dir);
        if cfg.root_folder_id.is_empty() {
            return Err("Omanote has no working notebook yet: pick one in the app".into());
        }
        Ok(Ctx { store: Mutex::new(Store::open(db).map_err(e)?), dir, cfg })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }

    fn tree(&self) -> Result<Vec<String>, String> {
        self.db().folder_subtree(self.cfg.tree_root()).map_err(e)
    }

    pub fn folders(&self) -> Result<Vec<FolderOut>, String> {
        let all = self.db().folders().map_err(e)?;
        fn walk(all: &[omanote_core::store::Folder], parent: &str, depth: usize, out: &mut Vec<FolderOut>) {
            for f in all.iter().filter(|f| f.parent_id == parent) {
                out.push(FolderOut {
                    id: f.id.clone(),
                    parent_id: f.parent_id.clone(),
                    title: f.title.clone(),
                    note_count: f.note_count,
                    depth,
                });
                walk(all, &f.id, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        if self.cfg.whole_joplin {
            walk(&all, "", 0, &mut out);
        } else if let Some(root) = all.iter().find(|f| f.id == self.cfg.root_folder_id) {
            out.push(FolderOut {
                id: root.id.clone(),
                parent_id: root.parent_id.clone(),
                title: root.title.clone(),
                note_count: root.note_count,
                depth: 0,
            });
            walk(&all, &root.id, 1, &mut out);
        }
        Ok(out)
    }

    /// Resolves a folder id or name inside the working notebook.
    fn folder_id(&self, key: &str) -> Result<String, String> {
        let tree = self.tree()?;
        let k = key.to_lowercase();
        self.folders()?
            .into_iter()
            .find(|f| tree.contains(&f.id) && (f.id == key || f.title.to_lowercase() == k))
            .map(|f| f.id)
            .ok_or_else(|| format!("folder '{key}' not found in the working notebook"))
    }

    /// Resolves a note id or unique id prefix inside the working notebook.
    fn note_id(&self, key: &str) -> Result<String, String> {
        let tree = self.tree()?;
        let notes = self.db().notes_in(&tree).map_err(e)?;
        let hits: Vec<_> = notes.iter().filter(|n| n.id.starts_with(key)).collect();
        match hits.as_slice() {
            [one] => Ok(one.id.clone()),
            [] => Err(format!("note '{key}' not found")),
            _ => Err(format!("'{key}' matches {} notes: use more characters", hits.len())),
        }
    }

    pub fn list(&self, folder: Option<&str>, limit: usize) -> Result<Vec<NoteSummary>, String> {
        let ids = match folder {
            Some(f) => vec![self.folder_id(f)?],
            None => self.tree()?,
        };
        let mut notes = self.db().notes_in(&ids).map_err(e)?;
        notes.truncate(limit);
        Ok(notes)
    }

    pub fn search(&self, query: &str) -> Result<Vec<NoteSummary>, String> {
        let tree = self.tree()?;
        self.db().search_notes(&tree, query).map_err(e)
    }

    pub fn read(&self, key: &str) -> Result<Note, String> {
        let id = self.note_id(key)?;
        self.db().note(&id).map_err(e)?.ok_or_else(|| "note not found".into())
    }

    pub fn create(&self, folder: Option<&str>, text: &str) -> Result<Note, String> {
        let parent = match folder {
            Some(f) => self.folder_id(f)?,
            None => self.cfg.root_folder_id.clone(),
        };
        self.db().create_note(&parent, text).map_err(e)
    }

    pub fn update(&self, key: &str, text: &str) -> Result<Note, String> {
        let id = self.note_id(key)?;
        self.db().update_note_text(&id, text).map_err(e)
    }

    pub fn append(&self, key: &str, text: &str) -> Result<Note, String> {
        let id = self.note_id(key)?;
        self.db().append_to_note(&id, text).map_err(e)
    }

    pub fn move_note(&self, key: &str, folder: &str) -> Result<(), String> {
        let (id, folder) = (self.note_id(key)?, self.folder_id(folder)?);
        self.db().move_note(&id, &folder).map_err(e)
    }

    pub fn trash(&self, key: &str) -> Result<(), String> {
        let id = self.note_id(key)?;
        self.db().trash(&id).map_err(e)
    }

    /// Syncs with Joplin Server using the credentials the app saved in the keychain.
    pub fn sync(&self) -> Result<SyncReport, String> {
        let password = secrets::get(&self.dir, secrets::SERVER_PASSWORD)
            .ok_or("no saved Joplin Server password: connect the Omanote app first")?;
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(e)?;
        rt.block_on(async {
            let mut api = JoplinServer::new(&self.cfg.server_url, &self.cfg.email, &password);
            let mut keys = KeyRing::new();
            let mut sync = Synchronizer {
                api: &mut api,
                store: &self.store,
                keys: &mut keys,
                client_id: format!("{}cli", &self.cfg.client_id.get(..29).unwrap_or("omanotecli")),
                client_type: LOCK_CLIENT_DESKTOP,
            };
            let info = sync.fetch_info().await.map_err(e)?;
            if info.e2ee_enabled() {
                if let Some(master) = secrets::get(&self.dir, secrets::MASTER_PASSWORD) {
                    unlock_keys(&info, &master, sync.keys);
                }
            }
            sync.sync().await.map_err(e)
        })
    }
}
