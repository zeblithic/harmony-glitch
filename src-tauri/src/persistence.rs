use std::io::Write;
use std::path::Path;

/// Atomically write `data` to `path` via temp file + fsync + rename.
///
/// On Unix, `unix_mode` (e.g. `Some(0o600)`) sets file permissions on the
/// temp file *before* the rename, so the final file is never world-readable
/// even momentarily.
///
/// If the process crashes mid-write, only the `.tmp` sibling is affected —
/// the original file at `path` remains intact.
pub fn atomic_write(path: &Path, data: &[u8], unix_mode: Option<u32>) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");

    {
        let mut f = std::fs::File::create(&tmp_path).map_err(|e| {
            format!("Failed to create temp file {}: {e}", tmp_path.display())
        })?;

        #[cfg(unix)]
        if let Some(mode) = unix_mode {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(mode);
            f.set_permissions(perms).map_err(|e| {
                format!("Failed to set permissions on {}: {e}", tmp_path.display())
            })?;
        }

        f.write_all(data).map_err(|e| {
            format!("Failed to write temp file {}: {e}", tmp_path.display())
        })?;
        f.sync_all().map_err(|e| {
            format!("Failed to fsync temp file {}: {e}", tmp_path.display())
        })?;
    }

    std::fs::rename(&tmp_path, path).map_err(|e| {
        format!(
            "Failed to rename {} → {}: {e}",
            tmp_path.display(),
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");
        let data = b"hello world";
        atomic_write(&path, data, None).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");
        atomic_write(&path, b"first", None).unwrap();
        atomic_write(&path, b"second", None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn tmp_file_does_not_linger() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");
        atomic_write(&path, b"data", None).unwrap();
        assert!(!dir.path().join("test.tmp").exists());
    }

    #[test]
    #[cfg(unix)]
    fn unix_permissions_applied() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("secret.json");
        atomic_write(&path, b"key material", Some(0o600)).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::mode(&metadata.permissions());
        assert_eq!(mode & 0o777, 0o600);
    }
}
