//! Cooperative cancel helpers for GUI hosts driving the litho CLI sidecar.
//!
//! pkexec elevates litho to root and does not reliably forward stdin, so Lithographer
//! passes `--cancel-file` and writes `cancel` into a file under the user's cache dir
//! (root can read it; the unprivileged parent can write it).

use crate::progress::{is_stdin_cancel_line, STDIN_CANCEL_LINE};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default directory for cancel flag files (`~/.cache/litho`).
pub fn cancel_cache_dir() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".cache/litho"))
}

/// Allocate a unique cancel flag path and create an empty marker file.
pub fn create_cancel_file() -> io::Result<PathBuf> {
    let dir = cancel_cache_dir()?;
    std::fs::create_dir_all(&dir)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("cancel-{}-{nanos}.flag", std::process::id()));
    init_cancel_file(&path)?;
    Ok(path)
}

/// Create or truncate the cancel marker file (not yet cancelled).
pub fn init_cancel_file(path: &Path) -> io::Result<()> {
    std::fs::write(path, "")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// Request cancel by writing the marker line (readable by an elevated litho child).
pub fn request_cancel_via_file(path: &Path) -> io::Result<()> {
    std::fs::write(path, format!("{STDIN_CANCEL_LINE}\n"))
}

/// Returns true when the cancel file contains the marker line.
pub fn cancel_requested_in_file(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|contents| is_stdin_cancel_line(&contents))
}

/// Best-effort removal of a spent cancel flag file.
pub fn remove_cancel_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_file_marker_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "litho-cancel-lib-test-{}-{}.flag",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        init_cancel_file(&path).unwrap();
        assert!(!cancel_requested_in_file(&path));
        request_cancel_via_file(&path).unwrap();
        assert!(cancel_requested_in_file(&path));
        remove_cancel_file(&path);
    }
}