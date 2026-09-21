mod secrets;
mod theme;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use omanote_core::api::JoplinServer;
use omanote_core::e2ee::KeyRing;
use omanote_core::store::{Folder, Note, NoteSummary, Store};
use omanote_core::sync::{unlock_keys, SyncInfo, SyncReport, Synchronizer};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

const SECRET_SERVER: &str = "server_password";
const SECRET_MASTER: &str = "master_password";

#[cfg(mobile)]
const CLIENT_TYPE: i64 = omanote_core::api::LOCK_CLIENT_MOBILE;
#[cfg(not(mobile))]
const CLIENT_TYPE: i64 = omanote_core::api::LOCK_CLIENT_DESKTOP;

fn default_hotkey() -> String {
    "CommandOrControl+Shift+Space".into()
}
fn default_interval() -> u64 {
    120
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    email: String,
    /// Joplin notebook used as Omanote's root (its sub-notebooks are the folders).
    #[serde(default)]
    root_folder_id: String,
    #[serde(default)]
    client_id: String,
    #[serde(default = "default_hotkey")]
    hotkey: String,
    #[serde(default = "default_interval")]
    sync_interval_secs: u64,
}

#[derive(Default)]
struct SyncCtx {
    api: Option<JoplinServer>,
    keys: KeyRing,
    info: Option<SyncInfo>,
}

struct AppState {
    dir: PathBuf,
    store: Arc<Mutex<Store>>,
    config: Mutex<Config>,
    sync: tokio::sync::Mutex<SyncCtx>,
}

impl AppState {
    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    fn save_config(&self, c: Config) -> Result<(), String> {
        std::fs::write(self.dir.join("config.json"), serde_json::to_string_pretty(&c).unwrap())
            .map_err(|e| e.to_string())?;
        *self.config.lock().unwrap() = c;
        Ok(())
    }

    fn root_tree(&self) -> Result<Vec<String>, String> {
        let root = self.config().root_folder_id;
        if root.is_empty() {
            return Ok(vec![]);
        }
        self.db().folder_subtree(&root).map_err(err)
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
struct Status {
    configured: bool,
    server_url: String,
    email: String,
    root_folder_id: String,
    e2ee: bool,
    locked: bool,
    hotkey: String,
    mobile: bool,
}

#[derive(Serialize, Clone)]
struct SyncEvent {
    state: &'static str,
    message: String,
    report: Option<SyncReport>,
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

async fn do_sync(app: &AppHandle) -> Result<Option<SyncReport>, String> {
    let state = app.state::<AppState>();
    let Ok(mut ctx) = state.sync.try_lock() else {
        return Ok(None); // already syncing
    };
    let cfg = state.config();
    if cfg.server_url.is_empty() {
        return Ok(None);
    }
    if ctx.api.is_none() {
        let Some(pw) = secrets::get(&state.dir, SECRET_SERVER) else { return Ok(None) };
        ctx.api = Some(JoplinServer::new(&cfg.server_url, &cfg.email, &pw));
    }

    let _ = app.emit("sync-status", SyncEvent { state: "syncing", message: String::new(), report: None });

    let SyncCtx { api, keys, info } = &mut *ctx;
    let mut sync = Synchronizer {
        api: api.as_mut().unwrap(),
        store: &state.store,
        keys,
        client_id: cfg.client_id.clone(),
        client_type: CLIENT_TYPE,
    };

    let result = async {
        let fetched = sync.fetch_info().await?;
        if fetched.e2ee_enabled() && sync.keys.active_id.is_none() {
            if let Some(master) = secrets::get(&state.dir, SECRET_MASTER) {
                unlock_keys(&fetched, &master, sync.keys);
            }
        }
        *info = Some(fetched);
        sync.sync().await
    }
    .await;

    match result {
        Ok(report) => {
            let changed = report.downloaded + report.deleted_local + report.conflicts > 0;
            let _ = app.emit(
                "sync-status",
                SyncEvent { state: "idle", message: String::new(), report: Some(report.clone()) },
            );
            if changed {
                let _ = app.emit("data-changed", ());
            }
            Ok(Some(report))
        }
        Err(e) => {
            let _ = app.emit("sync-status", SyncEvent { state: "error", message: e.to_string(), report: None });
            Err(e.to_string())
        }
    }
}

fn spawn_sync(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = do_sync(&app).await;
    });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The palette Omanote should use: `Some` only on Omarchy systems.
#[tauri::command]
fn get_system_theme() -> Option<theme::OmarchyTheme> {
    theme::current()
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<Status, String> {
    let c = state.config();
    let ctx = state.sync.lock().await;
    let e2ee = ctx.info.as_ref().map(|i| i.e2ee_enabled()).unwrap_or(false);
    Ok(Status {
        configured: !c.server_url.is_empty(),
        server_url: c.server_url,
        email: c.email,
        root_folder_id: c.root_folder_id,
        e2ee,
        locked: e2ee && ctx.keys.active_id.is_none(),
        hotkey: c.hotkey,
        mobile: cfg!(mobile),
    })
}

#[tauri::command]
async fn setup(
    app: AppHandle,
    state: State<'_, AppState>,
    server_url: String,
    email: String,
    password: String,
    master_password: Option<String>,
) -> Result<(), String> {
    let server_url = server_url.trim().trim_end_matches('/').to_string();
    let mut api = JoplinServer::new(&server_url, email.trim(), &password);
    api.login().await.map_err(|e| format!("Accesso non riuscito: {e}"))?;

    let mut keys = KeyRing::new();
    let info = {
        let mut sync = Synchronizer {
            api: &mut api,
            store: &state.store,
            keys: &mut keys,
            client_id: String::new(),
            client_type: CLIENT_TYPE,
        };
        sync.fetch_info().await.map_err(err)?
    };
    let master = master_password.filter(|m| !m.is_empty());
    if info.e2ee_enabled() {
        let Some(m) = &master else {
            return Err("E2EE_REQUIRED".into());
        };
        unlock_keys(&info, m, &mut keys);
        if keys.active_id.is_none() {
            return Err("Password master E2EE non corretta".into());
        }
    }

    let mut cfg = state.config();
    let server_changed = cfg.server_url != server_url || cfg.email != email.trim();
    if server_changed {
        state.db().reset().map_err(err)?;
        cfg.root_folder_id.clear();
    }
    cfg.server_url = server_url;
    cfg.email = email.trim().to_string();
    if cfg.client_id.is_empty() {
        cfg.client_id = omanote_core::item::new_id();
    }
    secrets::set(&state.dir, SECRET_SERVER, Some(&password))?;
    secrets::set(&state.dir, SECRET_MASTER, master.as_deref())?;
    state.save_config(cfg)?;

    {
        let mut ctx = state.sync.lock().await;
        *ctx = SyncCtx { api: Some(api), keys, info: Some(info) };
    }
    do_sync(&app).await?;
    Ok(())
}

#[tauri::command]
async fn unlock(app: AppHandle, state: State<'_, AppState>, master_password: String) -> Result<(), String> {
    {
        let mut guard = state.sync.lock().await;
        let ctx = &mut *guard;
        let info = ctx.info.clone().ok_or("Sincronizza prima di sbloccare")?;
        unlock_keys(&info, &master_password, &mut ctx.keys);
        if ctx.keys.active_id.is_none() {
            return Err("Password master E2EE non corretta".into());
        }
    }
    secrets::set(&state.dir, SECRET_MASTER, Some(&master_password))?;
    spawn_sync(&app);
    Ok(())
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    *state.sync.lock().await = SyncCtx::default();
    secrets::set(&state.dir, SECRET_SERVER, None)?;
    secrets::set(&state.dir, SECRET_MASTER, None)?;
    state.db().reset().map_err(err)?;
    let mut cfg = state.config();
    cfg.server_url.clear();
    cfg.email.clear();
    cfg.root_folder_id.clear();
    state.save_config(cfg)
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> Result<Option<SyncReport>, String> {
    do_sync(&app).await
}

#[tauri::command]
fn set_root_folder(app: AppHandle, state: State<'_, AppState>, folder_id: Option<String>, new_title: Option<String>) -> Result<String, String> {
    let id = match (folder_id, new_title) {
        (Some(id), _) if !id.is_empty() => id,
        (_, Some(t)) => state.db().create_folder(&t, "").map_err(err)?.id,
        _ => return Err("Scegli o crea un notebook".into()),
    };
    let mut cfg = state.config();
    cfg.root_folder_id = id.clone();
    state.save_config(cfg)?;
    spawn_sync(&app);
    Ok(id)
}

#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, String> {
    state.db().folders().map_err(err)
}

/// Notes of one folder, or of the whole Omanote tree when `folder_id` is empty.
#[tauri::command]
fn list_notes(state: State<'_, AppState>, folder_id: Option<String>) -> Result<Vec<NoteSummary>, String> {
    let ids = match folder_id.filter(|f| !f.is_empty()) {
        Some(f) => vec![f],
        None => state.root_tree()?,
    };
    state.db().notes_in(&ids).map_err(err)
}

#[tauri::command]
fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<NoteSummary>, String> {
    let ids = state.root_tree()?;
    state.db().search_notes(&ids, &query).map_err(err)
}

#[tauri::command]
fn get_note(state: State<'_, AppState>, id: String) -> Result<Option<Note>, String> {
    state.db().note(&id).map_err(err)
}

#[tauri::command]
fn create_note(state: State<'_, AppState>, folder_id: Option<String>, text: String) -> Result<Note, String> {
    let parent = folder_id.filter(|f| !f.is_empty()).unwrap_or_else(|| state.config().root_folder_id);
    if parent.is_empty() {
        return Err("Nessun notebook selezionato".into());
    }
    state.db().create_note(&parent, &text).map_err(err)
}

#[tauri::command]
fn update_note(state: State<'_, AppState>, id: String, text: String) -> Result<Note, String> {
    state.db().update_note_text(&id, &text).map_err(err)
}

#[tauri::command]
fn move_note(state: State<'_, AppState>, id: String, folder_id: String) -> Result<(), String> {
    state.db().move_note(&id, &folder_id).map_err(err)
}

#[tauri::command]
fn trash_note(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.db().trash(&id).map_err(err)
}

#[tauri::command]
fn create_folder(state: State<'_, AppState>, title: String, parent_id: Option<String>) -> Result<Folder, String> {
    let parent = parent_id.filter(|p| !p.is_empty()).unwrap_or_else(|| state.config().root_folder_id);
    state.db().create_folder(&title, &parent).map_err(err)
}

#[tauri::command]
fn rename_folder(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    state.db().update_folder(&id, Some(&title), None).map_err(err)
}

#[tauri::command]
fn move_folder(state: State<'_, AppState>, id: String, parent_id: String) -> Result<(), String> {
    state.db().update_folder(&id, None, Some(&parent_id)).map_err(err)
}

#[tauri::command]
fn trash_folder(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if id == state.config().root_folder_id {
        return Err("Non puoi eliminare il notebook principale di Omanote".into());
    }
    let db = state.db();
    let tree = db.folder_subtree(&id).map_err(err)?;
    for note in db.notes_in(&tree).map_err(err)? {
        db.trash(&note.id).map_err(err)?;
    }
    for f in tree.iter().rev() {
        db.trash(f).map_err(err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Desktop: window toggling, tray, global hotkey
// ---------------------------------------------------------------------------

#[cfg(desktop)]
fn show_window(app: &AppHandle, quick_note: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if quick_note {
            let _ = app.emit("quick-note", ());
        }
    }
}

#[cfg(desktop)]
fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_window(app, true);
        }
    }
}

#[cfg(desktop)]
fn setup_desktop(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let new_note = MenuItem::with_id(app, "new", "Nuova nota", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Mostra Omanote", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sincronizza ora", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&new_note, &show, &sync, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Omanote")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "new" => show_window(app, true),
            "show" => show_window(app, false),
            "sync" => spawn_sync(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                toggle_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon_as_template(false);
    }
    tray.build(app)?;

    let hotkey = app.state::<AppState>().config().hotkey;
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_window(app);
                }
            })
            .build(),
    )?;
    if let Err(e) = app.global_shortcut().register(hotkey.as_str()) {
        log::warn!("cannot register global shortcut {hotkey}: {e}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(dir.join("omanote.sqlite")).map_err(|e| e.to_string())?;
            let config: Config = std::fs::read_to_string(dir.join("config.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| Config {
                    hotkey: default_hotkey(),
                    sync_interval_secs: default_interval(),
                    ..Default::default()
                });
            let interval = config.sync_interval_secs.max(30);
            app.manage(AppState {
                dir,
                store: Arc::new(Mutex::new(store)),
                config: Mutex::new(config),
                sync: tokio::sync::Mutex::new(SyncCtx::default()),
            });

            #[cfg(desktop)]
            setup_desktop(app)?;

            // Follow the Omarchy theme while the app runs.
            if let Some(initial) = theme::current() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut last = initial;
                    loop {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        if let Some(t) = theme::current() {
                            if t != last {
                                last = t.clone();
                                let _ = handle.emit("theme-changed", t);
                            }
                        }
                    }
                });
            }

            // Periodic background sync.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let _ = do_sync(&handle).await;
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Desktop: closing the window keeps Omanote in the menu bar / tray.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            #[cfg(mobile)]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_system_theme,
            setup,
            unlock,
            logout,
            sync_now,
            set_root_folder,
            list_folders,
            list_notes,
            search_notes,
            get_note,
            create_note,
            update_note,
            move_note,
            trash_note,
            create_folder,
            rename_folder,
            move_folder,
            trash_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Omanote");

    app.run(|_app, _event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            show_window(_app, false);
        }
    });
}
