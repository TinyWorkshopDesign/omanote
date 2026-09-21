//! Settings shared by the app, the CLI and the MCP server (`config.json` in
//! the data directory). Passwords are not here: see [`crate::secrets`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Must match the Tauri bundle identifier: the app keeps its data in
/// `<platform data dir>/app.omanote`.
pub const APP_ID: &str = "app.omanote";
pub const DB_FILE: &str = "omanote.sqlite";

fn default_hotkey() -> String {
    // Default global hotkey.
    "Alt+A".into()
}
fn default_interval() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub email: String,
    /// Joplin notebook used as Omanote's root (its sub-notebooks are the folders).
    #[serde(default)]
    pub root_folder_id: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_interval")]
    pub sync_interval_secs: u64,
    /// Local mode: notes only on this device, no Joplin Server (yet). Connecting a
    /// server later uploads the local notes instead of discarding them.
    #[serde(default)]
    pub local_only: bool,
    /// Work on every Joplin notebook instead of `root_folder_id` only. New notes
    /// still go to `root_folder_id` (Joplin notes always live in a notebook).
    #[serde(default)]
    pub whole_joplin: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            email: String::new(),
            root_folder_id: String::new(),
            client_id: String::new(),
            hotkey: default_hotkey(),
            sync_interval_secs: default_interval(),
            local_only: false,
            whole_joplin: false,
        }
    }
}

impl Config {
    /// Top of the tree Omanote shows: the working notebook, or "" (all of Joplin).
    pub fn tree_root(&self) -> &str {
        if self.whole_joplin { "" } else { &self.root_folder_id }
    }

    /// Ready to use: connected to a server, or explicitly in local mode.
    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty() || self.local_only
    }

    pub fn load(dir: &Path) -> Config {
        std::fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(self).expect("config json"))
    }
}

/// Desktop data directory, `$OMANOTE_DATA_DIR` overriding it (used by the CLI).
pub fn default_data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("OMANOTE_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_ID)
}
