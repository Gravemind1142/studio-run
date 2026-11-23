//! A per-run temporary directory, removed afterwards unless `--keep-temp`.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TempDir {
    /// Unique per run; also identifies the run in Studio's log. Only the
    /// local host reads it, so it goes unread on Linux.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    pub id: String,
    pub path: PathBuf,
    keep: bool,
}

impl TempDir {
    pub fn create(keep: bool) -> Result<TempDir, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("run-{}-{:x}", std::process::id(), nanos);
        let path = std::env::temp_dir().join("studio-run").join(&id);
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("couldn't create {}: {e}", path.display()))?;
        Ok(TempDir { id, path, keep })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
