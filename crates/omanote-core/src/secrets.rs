//! Credentials storage: the OS keychain on macOS/iOS/Linux, a private file
//! inside the app sandbox on Android. Shared by the app, the CLI and the MCP server.

use std::path::Path;

const SERVICE: &str = "app.omanote";

pub const SERVER_PASSWORD: &str = "server_password";
pub const MASTER_PASSWORD: &str = "master_password";

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
    write_private(&file(dir), serde_json::to_string(&m).unwrap().as_bytes()).map_err(|e| e.to_string())
}

/// Writes a file readable and writable by its owner only (0600), also tightening
/// a file created earlier with the default mode. The app sandbox already keeps
/// other apps out; this keeps the passwords out of anything that copies the data
/// directory with its permissions. Backups are disabled in the Android manifest.
#[cfg(unix)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn private_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("omanote-secrets-{}", crate::item::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.json");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::write_private(&path, b"{}").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
