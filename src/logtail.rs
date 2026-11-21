//! Following Studio's own log file, which (unlike `--outputFile`) is written
//! while Studio runs. Each Studio process writes its own file under
//! `%LOCALAPPDATA%\Roblox\logs`, and its header contains the command line, so
//! we find ours by looking for a marker unique to this run.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct LogTail {
    dir: PathBuf,
    marker: String,
    since: SystemTime,
    path: Option<PathBuf>,
    offset: u64,
    partial: Vec<u8>,
    /// From Studio's "Authenticated : YES/NO" lines. NO is logged briefly at
    /// startup even when a saved login works, so only NO-without-YES matters.
    pub authenticated: Option<bool>,
}

impl LogTail {
    pub fn new(marker: &str) -> LogTail {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join(r"Roblox\logs"))
            .unwrap_or_default();
        LogTail {
            dir,
            marker: marker.to_string(),
            since: SystemTime::now() - Duration::from_secs(5),
            path: None,
            offset: 0,
            partial: Vec::new(),
            authenticated: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// New complete lines since the last call.
    pub fn poll(&mut self) -> Vec<String> {
        if self.path.is_none() {
            self.path = self.find();
        }
        let Some(path) = &self.path else {
            return vec![];
        };
        let mut buf = Vec::new();
        let read = File::open(path).and_then(|mut f| {
            f.seek(SeekFrom::Start(self.offset))?;
            f.read_to_end(&mut buf)
        });
        let Ok(n) = read else { return vec![] };
        self.offset += n as u64;
        self.partial.extend_from_slice(&buf);

        let mut lines = Vec::new();
        while let Some(i) = self.partial.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.partial.drain(..=i).collect();
            let line = String::from_utf8_lossy(&raw).trim_end().to_string();
            if line.contains("Authenticated : YES") {
                self.authenticated = Some(true);
            } else if line.contains("Authenticated : NO") && self.authenticated.is_none() {
                self.authenticated = Some(false);
            }
            lines.push(line);
        }
        lines
    }

    fn find(&self) -> Option<PathBuf> {
        let entries = std::fs::read_dir(&self.dir).ok()?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.contains("Studio") || !name.ends_with(".log") || name.contains("CrashHandler")
            {
                continue;
            }
            let fresh = e
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t >= self.since);
            if !fresh {
                continue;
            }
            let mut head = vec![0u8; 16 * 1024];
            let n = File::open(e.path())
                .and_then(|mut f| f.read(&mut head))
                .unwrap_or(0);
            if String::from_utf8_lossy(&head[..n]).contains(&self.marker) {
                return Some(e.path());
            }
        }
        None
    }
}
