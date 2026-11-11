//! Desktop self-update from the GitHub releases (`tauri-plugin-updater`).
//!
//! The release workflow signs the bundles and publishes `latest.json` next to them;
//! the app reads it from `/releases/latest/download/` (see `plugins.updater` in
//! `tauri.conf.json`) and only installs a bundle whose signature matches the public
//! key compiled into the app.
//!
//! What can be replaced in place: the macOS `.app` and a Linux AppImage. A `.deb`,
//! `.rpm` or distribution package belongs to its package manager, and a build from
//! source has nothing to replace: there the app only says that a newer version exists.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

const E_UPDATE: &str = "UPDATE_FAILED";

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    /// The release notes, as written on GitHub (may be empty).
    notes: String,
    /// False where the running app cannot replace itself: the UI links to the download.
    can_install: bool,
}

/// Whether this copy of the app can be replaced in place.
fn can_install() -> bool {
    self_updating(std::env::consts::OS, std::env::var_os("APPIMAGE").is_some())
}

fn self_updating(os: &str, appimage: bool) -> bool {
    match os {
        "macos" => true,
        "linux" => appimage,
        _ => false,
    }
}

fn fail(e: impl std::fmt::Display) -> String {
    format!("{E_UPDATE}|{e}")
}

/// The newer version published on GitHub, if there is one.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app.updater().map_err(fail)?.check().await.map_err(fail)?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default(),
        can_install: can_install(),
    }))
}

/// Downloads the newer version, verifies its signature, replaces the app and restarts it.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if !can_install() {
        return Err(fail("this build is not self-updating"));
    }
    let Some(update) = app.updater().map_err(fail)?.check().await.map_err(fail)? else {
        return Ok(());
    };
    update.download_and_install(|_, _| {}, || {}).await.map_err(fail)?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::self_updating;

    #[test]
    fn only_app_bundles_and_appimages_update_themselves() {
        assert!(self_updating("macos", false));
        assert!(self_updating("linux", true));
        assert!(!self_updating("linux", false)); // .deb, .rpm, built from source
        assert!(!self_updating("windows", false)); // no Windows bundle is published
    }
}
