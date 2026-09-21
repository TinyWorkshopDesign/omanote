mod ocr;
mod theme;
mod timer;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use omanote_core::api::JoplinServer;
use omanote_core::config::Config;
use omanote_core::e2ee::KeyRing;
use omanote_core::secrets;
use omanote_core::store::{Folder, Note, NoteSummary, Store};
use omanote_core::sync::{unlock_keys, SyncInfo, SyncReport, Synchronizer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(mobile)]
const CLIENT_TYPE: i64 = omanote_core::api::LOCK_CLIENT_MOBILE;
#[cfg(not(mobile))]
const CLIENT_TYPE: i64 = omanote_core::api::LOCK_CLIENT_DESKTOP;

// Errors reach the UI as `CODE` or `CODE|detail`; the UI translates the code.
const E_LOGIN: &str = "LOGIN_FAILED";
const E_E2EE_REQUIRED: &str = "E2EE_REQUIRED";
const E_BAD_MASTER: &str = "BAD_MASTER_PASSWORD";
const E_SYNC_FIRST: &str = "SYNC_FIRST";
const E_NO_NOTEBOOK: &str = "NO_NOTEBOOK";
const E_ROOT_NOTEBOOK: &str = "ROOT_NOTEBOOK";

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
    timer: Mutex<Option<timer::Timer>>,
    /// UI language, for the few strings produced by Rust (notifications).
    lang: Mutex<String>,
}

impl AppState {
    fn db(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    fn save_config(&self, c: Config) -> Result<(), String> {
        c.save(&self.dir).map_err(err)?;
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

fn now_ms() -> i64 {
    omanote_core::item::now_ms()
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
    /// "macos", "linux", "ios", "android", …
    platform: &'static str,
    /// Running on Omarchy (the global hotkey is then a Hyprland binding).
    omarchy: bool,
    data_dir: String,
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
        let Some(pw) = secrets::get(&state.dir, secrets::SERVER_PASSWORD) else { return Ok(None) };
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
            if let Some(master) = secrets::get(&state.dir, secrets::MASTER_PASSWORD) {
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
// Account & notebooks
// ---------------------------------------------------------------------------

fn is_omarchy() -> bool {
    cfg!(target_os = "linux")
        && std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/omarchy").exists())
            .unwrap_or(false)
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
        platform: std::env::consts::OS,
        omarchy: is_omarchy(),
        data_dir: state.dir.display().to_string(),
    })
}

#[tauri::command]
fn set_language(state: State<'_, AppState>, lang: String) {
    *state.lang.lock().unwrap() = lang;
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
    api.login().await.map_err(|e| format!("{E_LOGIN}|{e}"))?;

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
            return Err(E_E2EE_REQUIRED.into());
        };
        unlock_keys(&info, m, &mut keys);
        if keys.active_id.is_none() {
            return Err(E_BAD_MASTER.into());
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
    secrets::set(&state.dir, secrets::SERVER_PASSWORD, Some(&password))?;
    secrets::set(&state.dir, secrets::MASTER_PASSWORD, master.as_deref())?;
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
        let info = ctx.info.clone().ok_or(E_SYNC_FIRST)?;
        unlock_keys(&info, &master_password, &mut ctx.keys);
        if ctx.keys.active_id.is_none() {
            return Err(E_BAD_MASTER.into());
        }
    }
    secrets::set(&state.dir, secrets::MASTER_PASSWORD, Some(&master_password))?;
    spawn_sync(&app);
    Ok(())
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    *state.sync.lock().await = SyncCtx::default();
    secrets::set(&state.dir, secrets::SERVER_PASSWORD, None)?;
    secrets::set(&state.dir, secrets::MASTER_PASSWORD, None)?;
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

/// Chooses (or creates) the Joplin notebook Omanote works in. It can be
/// changed at any time: notes stay where they are in Joplin.
#[tauri::command]
fn set_root_folder(app: AppHandle, state: State<'_, AppState>, folder_id: Option<String>, new_title: Option<String>) -> Result<String, String> {
    let id = match (folder_id, new_title) {
        (Some(id), _) if !id.is_empty() => id,
        (_, Some(t)) if !t.trim().is_empty() => state.db().create_folder(&t, "").map_err(err)?.id,
        _ => return Err(E_NO_NOTEBOOK.into()),
    };
    let mut cfg = state.config();
    cfg.root_folder_id = id.clone();
    state.save_config(cfg)?;
    spawn_sync(&app);
    Ok(id)
}

// ---------------------------------------------------------------------------
// Notes & folders
// ---------------------------------------------------------------------------

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
        return Err(E_NO_NOTEBOOK.into());
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
fn promote_note(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.db().promote(&id).map_err(err)
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
        return Err(E_ROOT_NOTEBOOK.into());
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
// Timer
// ---------------------------------------------------------------------------

fn notification_text(lang: &str, event: &str, title: &str) -> String {
    let (done, work, rest) = match lang {
        "it" => ("Tempo scaduto", "Fine lavoro: pausa!", "Fine pausa: al lavoro!"),
        "es" => ("Se acabó el tiempo", "Fin del trabajo: ¡descanso!", "Fin del descanso: ¡a trabajar!"),
        "fr" => ("Temps écoulé", "Fin du travail : pause !", "Fin de la pause : au travail !"),
        "de" => ("Zeit abgelaufen", "Arbeitszeit vorbei: Pause!", "Pause vorbei: weiter geht's!"),
        "pt" => ("O tempo acabou", "Fim do trabalho: pausa!", "Fim da pausa: ao trabalho!"),
        "nl" => ("De tijd is om", "Werk klaar: pauze!", "Pauze voorbij: aan het werk!"),
        "pl" => ("Czas minął", "Koniec pracy: przerwa!", "Koniec przerwy: do pracy!"),
        _ => ("Time's up", "Work done: take a break!", "Break over: back to work!"),
    };
    let msg = match event {
        "work_done" => work,
        "rest_done" => rest,
        _ => done,
    };
    if title.is_empty() {
        msg.to_string()
    } else {
        format!("{title} — {msg}")
    }
}

fn emit_timer(app: &AppHandle, t: Option<&timer::Timer>) {
    let tick = t.map(|t| t.tick(now_ms()));
    #[cfg(desktop)]
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(tick.as_ref().map(|t| t.label.as_str()));
    }
    let _ = app.emit("timer", tick);
}

/// Runs a `timer …` line. Returns false if the line is not a timer command.
#[tauri::command]
fn timer_command(app: AppHandle, state: State<'_, AppState>, line: String) -> bool {
    let Some(cmd) = timer::parse(&line) else { return false };
    let now = now_ms();
    let mut slot = state.timer.lock().unwrap();
    match cmd {
        timer::Command::Pause => {
            if let Some(t) = slot.as_mut() {
                t.toggle_pause(now);
            }
        }
        timer::Command::Restart => {
            if let Some(t) = slot.as_mut() {
                t.restart(now);
            }
        }
        timer::Command::Stop => *slot = None,
        start => *slot = timer::Timer::start(&start, now),
    }
    emit_timer(&app, slot.as_ref());
    true
}

#[tauri::command]
fn timer_toggle(app: AppHandle, state: State<'_, AppState>) {
    let mut slot = state.timer.lock().unwrap();
    if let Some(t) = slot.as_mut() {
        t.toggle_pause(now_ms());
    }
    emit_timer(&app, slot.as_ref());
}

#[tauri::command]
fn timer_stop(app: AppHandle, state: State<'_, AppState>) {
    let mut slot = state.timer.lock().unwrap();
    *slot = None;
    emit_timer(&app, None);
}

#[tauri::command]
fn timer_state(state: State<'_, AppState>) -> Option<timer::Tick> {
    state.timer.lock().unwrap().as_ref().map(|t| t.tick(now_ms()))
}

fn spawn_timer_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let state = app.state::<AppState>();
            let (finished, snapshot) = {
                let mut slot = state.timer.lock().unwrap();
                let Some(t) = slot.as_mut() else { continue };
                if !t.running {
                    continue;
                }
                (t.advance(now_ms()), t.clone())
            };
            emit_timer(&app, Some(&snapshot));
            if let Some(event) = finished {
                use tauri_plugin_notification::NotificationExt;
                let lang = state.lang.lock().unwrap().clone();
                let _ = app
                    .notification()
                    .builder()
                    .title("Omanote")
                    .body(notification_text(&lang, event, &snapshot.title))
                    .sound("default")
                    .show();
                let _ = app.emit("timer-finished", event);
                if event == "done" {
                    // A finished countdown disappears after a short while.
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(8)).await;
                        let st = app2.state::<AppState>();
                        let mut slot = st.timer.lock().unwrap();
                        if slot.as_ref().is_some_and(|t| !t.running && t.kind == timer::Kind::Countdown) {
                            *slot = None;
                            emit_timer(&app2, None);
                        }
                    });
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// OCR
// ---------------------------------------------------------------------------

/// Text from an image sent as the raw request body (paste / drop in the UI).
#[tauri::command]
async fn ocr_image(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("OCR_FAILED|expected raw image bytes".into());
    };
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || ocr::recognize(&bytes)).await.map_err(err)?
}

/// Text from an image file dropped on the window.
#[tauri::command]
async fn ocr_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("OCR_FAILED|{e}"))?;
        ocr::recognize(&bytes)
    })
    .await
    .map_err(err)?
}

/// Select a region of the screen and return its text (desktop).
#[tauri::command]
async fn capture_text(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(desktop)]
    let window = app.get_webview_window("main");
    #[cfg(desktop)]
    if let Some(w) = &window {
        let _ = w.hide();
    }
    let res = tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(Duration::from_millis(250));
        match ocr::capture_region()? {
            Some(png) => ocr::recognize(&png).map(Some),
            None => Ok(None),
        }
    })
    .await
    .map_err(err)?;
    #[cfg(desktop)]
    if let Some(w) = &window {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = &app;
    res
}

// ---------------------------------------------------------------------------
// Window (desktop): pin, hide, tray, global hotkey, single instance
// ---------------------------------------------------------------------------

#[tauri::command]
fn toggle_pin(window: tauri::WebviewWindow) -> Result<bool, String> {
    #[cfg(desktop)]
    {
        let pinned = !window.is_always_on_top().map_err(err)?;
        window.set_always_on_top(pinned).map_err(err)?;
        Ok(pinned)
    }
    #[cfg(mobile)]
    {
        let _ = window;
        Ok(false)
    }
}

/// macOS: shows or hides the traffic lights. Omanote has no title bar; the
/// window buttons appear together with the hover menu.
#[tauri::command]
fn set_window_controls(window: tauri::WebviewWindow, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let w = window.clone();
        let _ = window.run_on_main_thread(move || {
            use objc2_app_kit::{NSWindow, NSWindowButton};
            let Ok(ptr) = w.ns_window() else { return };
            // SAFETY: Tauri hands out the live NSWindow of this webview window,
            // and we are on the main thread as AppKit requires.
            let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            for b in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
                if let Some(button) = ns.standardWindowButton(b) {
                    button.setHidden(!visible);
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, visible);
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    #[cfg(desktop)]
    let _ = window.hide();
    #[cfg(mobile)]
    let _ = window;
}

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
            show_window(app, false);
        }
    }
}

/// `omanote --toggle` (Hyprland binding on Omarchy), `--new`, `--capture`.
#[cfg(desktop)]
fn handle_args(app: &AppHandle, args: &[String]) {
    if args.iter().any(|a| a == "--toggle") {
        toggle_window(app);
    } else if args.iter().any(|a| a == "--new") {
        show_window(app, true);
    } else if args.iter().any(|a| a == "--capture") {
        show_window(app, false);
        let _ = app.emit("capture-text", ());
    } else {
        show_window(app, false);
    }
}

#[cfg(desktop)]
fn setup_desktop(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let new_note = MenuItem::with_id(app, "new", "+  Omanote", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Omanote", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "⌖  OCR", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "⟳  Sync", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "⏻  Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&new_note, &show, &capture, &sync, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Omanote")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "new" => show_window(app, true),
            "show" => show_window(app, false),
            "capture" => {
                show_window(app, false);
                let _ = app.emit("capture-text", ());
            }
            "sync" => spawn_sync(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                toggle_window(tray.app_handle());
            }
        });
    // Monochrome pencil. On macOS it is a template image: the system tints it
    // white or black to match the menu bar, like its own icons.
    tray = tray.icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?);
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon_as_template(true);
    }
    tray.build(app)?;

    // Wayland (Omarchy) does not let apps grab global keys: there the hotkey
    // is a Hyprland binding running `omanote --toggle` (see Settings).
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
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        if let Err(e) = app.global_shortcut().register(hotkey.as_str()) {
            log::warn!("cannot register global shortcut {hotkey}: {e}");
        }
    }
    Ok(())
}

/// Other processes (CLI, MCP server, AI agents) write to the same database:
/// refresh the UI when they do.
fn spawn_external_change_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last = app.state::<AppState>().db().data_version().unwrap_or(0);
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let v = app.state::<AppState>().db().data_version().unwrap_or(last);
            if v != last {
                last = v;
                let _ = app.emit("data-changed", ());
            }
        }
    });
}

/// Debug builds only: JS errors and diagnostics from the webview end up in the
/// `tauri dev` log, and `<data dir>/debug-eval.js` (if present) is run in the
/// window, then deleted. Lets tooling inspect the real WKWebView/WebKitGTK.
#[cfg(debug_assertions)]
#[tauri::command]
fn debug_log(msg: String) {
    eprintln!("[webview] {msg}");
}

#[cfg(debug_assertions)]
fn spawn_debug_eval(app: AppHandle) {
    let path = app.state::<AppState>().dir.join("debug-eval.js");
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Ok(js) = std::fs::read_to_string(&path) {
                let _ = std::fs::remove_file(&path);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.eval(&js);
                }
            }
        }
    });
}

/// The palette Omanote should use: `Some` only on Omarchy systems.
#[tauri::command]
fn get_system_theme() -> Option<theme::OmarchyTheme> {
    theme::current()
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Must be the first plugin: a second launch forwards its args and exits.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_args(app, &args);
        }));
    }

    let app = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(dir.join(omanote_core::config::DB_FILE)).map_err(|e| e.to_string())?;
            let config = Config::load(&dir);
            let interval = config.sync_interval_secs.max(30);
            app.manage(AppState {
                dir,
                store: Arc::new(Mutex::new(store)),
                config: Mutex::new(config),
                sync: tokio::sync::Mutex::new(SyncCtx::default()),
                timer: Mutex::new(None),
                lang: Mutex::new("en".into()),
            });

            #[cfg(desktop)]
            setup_desktop(app)?;

            // No title bar: macOS keeps the traffic lights (hidden until the hover
            // menu shows), Linux drops the decorations (Hyprland has none anyway).
            if let Some(w) = app.get_webview_window("main") {
                #[cfg(target_os = "linux")]
                let _ = w.set_decorations(false);
                set_window_controls(w, false);
            }

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

            spawn_timer_loop(app.handle().clone());
            #[cfg(debug_assertions)]
            spawn_debug_eval(app.handle().clone());
            spawn_external_change_watch(app.handle().clone());

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
            set_language,
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
            promote_note,
            trash_note,
            create_folder,
            rename_folder,
            move_folder,
            trash_folder,
            timer_command,
            timer_toggle,
            timer_stop,
            timer_state,
            ocr_image,
            ocr_file,
            capture_text,
            toggle_pin,
            hide_window,
            set_window_controls,
            #[cfg(debug_assertions)]
            debug_log,
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
