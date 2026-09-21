//! Credentials storage: the OS keychain on macOS/iOS/Linux, a private file
//! inside the app sandbox on Android.

use std::path::Path;

const SERVICE: &str = "app.omanote";

#[cfg(not(target_os = "android"))]
pub fn get(_dir: &Path, key: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, key).ok()?.get_password().ok()
}

#[cfg(not(target_os = "android"))]
pub fn set(_dir: &Path, key: &str, value: Option<&str>) -> Result<(), String> {
    let e = keyring::Entry::new(SERVICE, key).map_err(|e| e.to_string())?;
    match value {
        Some(v) => e.set_password(v).map_err(|e| e.to_string()),
        None => {
            let _ = e.delete_credential();
            Ok(())
        }
    }
}

#[cfg(target_os = "android")]
fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("secrets.json")
}

#[cfg(target_os = "android")]
fn load(dir: &Path) -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(file(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
pub fn get(dir: &Path, key: &str) -> Option<String> {
    let _ = SERVICE;
    load(dir).remove(key)
}

#[cfg(target_os = "android")]
pub fn set(dir: &Path, key: &str, value: Option<&str>) -> Result<(), String> {
    let mut m = load(dir);
    match value {
        Some(v) => m.insert(key.to_string(), v.to_string()),
        None => m.remove(key),
    };
    std::fs::write(file(dir), serde_json::to_string(&m).unwrap()).map_err(|e| e.to_string())
}
