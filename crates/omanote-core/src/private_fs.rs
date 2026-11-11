//! Owner-only files and directories for local data: the database, decrypted
//! attachments, logs, config and secrets hold the user's notes, so on Unix other
//! local users must not read them, whatever the umask. Directories get 0700 and
//! files 0600, at creation and when an existing install is opened. Elsewhere
//! these are plain `std::fs` calls (Windows keeps per-user ACLs on the data dir).

use std::io;
use std::path::Path;

#[cfg(unix)]
const DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// Creates `dir` and its missing parents as owner-only directories, and tightens
/// `dir` itself if it already existed (installs made before this check).
pub fn create_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(dir)?;
        set_mode(dir, DIR_MODE)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Writes `bytes` to a file readable and writable by its owner only, tightening a
/// file created earlier with the default mode.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(FILE_MODE).open(path)?;
        // `mode` applies only when the file is created.
        f.set_permissions(std::fs::Permissions::from_mode(FILE_MODE))?;
        f.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// Creates an empty owner-only file if `path` does not exist yet, so a library
/// that then opens it (SQLite) keeps that mode. An existing file is left as is.
pub fn create_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(FILE_MODE).open(path) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            r => r.map(drop),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Tightens an existing file (0600) or directory (0700); a missing path is fine.
/// For files other code created, and for data left by older versions.
pub fn restrict(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if meta.file_type().is_symlink() {
            return Ok(()); // never chmod whatever a link points to
        }
        set_mode(path, if meta.is_dir() { DIR_MODE } else { FILE_MODE })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// `restrict` on a directory and on each entry directly inside it.
pub fn restrict_dir_and_files(dir: &Path) -> io::Result<()> {
    restrict(dir)?;
    #[cfg(unix)]
    {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        for entry in entries {
            restrict(&entry?.path())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let current = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if current == mode {
        return Ok(());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    pub fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    pub fn loosen(path: &Path, m: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(m)).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omanote-perm-{name}-{}", crate::item::new_id()))
    }

    #[test]
    fn new_and_existing_directories_are_owner_only() {
        let root = scratch("dir");
        let dir = root.join("a").join("b");
        super::create_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&root.join("a")), 0o700, "created parents too");
        loosen(&dir, 0o755);
        super::create_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700, "an existing directory is tightened");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn written_files_are_owner_only() {
        let dir = scratch("write");
        super::create_dir(&dir).unwrap();
        let path = dir.join("f");
        std::fs::write(&path, "old").unwrap();
        loosen(&path, 0o644);
        super::write(&path, b"new").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let fresh = dir.join("g");
        super::write(&fresh, b"x").unwrap();
        assert_eq!(mode(&fresh), 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_file_and_restrict() {
        let dir = scratch("restrict");
        std::fs::create_dir_all(&dir).unwrap();
        loosen(&dir, 0o755);
        let path = dir.join("db");
        super::create_file(&path).unwrap();
        assert_eq!(mode(&path), 0o600);
        std::fs::write(&path, "data").unwrap();
        super::create_file(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"data", "an existing file is kept");

        let other = dir.join("other");
        std::fs::write(&other, "x").unwrap();
        loosen(&other, 0o644);
        super::restrict_dir_and_files(&dir).unwrap();
        assert_eq!((mode(&dir), mode(&path), mode(&other)), (0o700, 0o600, 0o600));
        super::restrict(&dir.join("missing")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
