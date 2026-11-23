//! Launching Studio with `--background`, in its own process group, so a
//! timeout or our own exit kills Studio and the processes it started.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::WindowInfo;

const SIGKILL: i32 = 9;

/// A running Studio process group. Dropping it kills the group.
pub struct Studio {
    child: Mutex<Child>,
    /// Process-group id. `process_group(0)` makes this the child's pid.
    pgid: i32,
    status: Mutex<Option<ExitStatus>>,
}

impl Studio {
    /// Start `exe args...`. With `hidden`, pass Studio's `--background` flag.
    pub fn launch(exe: &Path, args: &[String], hidden: bool) -> Result<Studio, String> {
        let mut cmd = Command::new(exe);
        if hidden {
            cmd.arg("--background");
        }
        // Null std handles: Studio's CLI logs must not mix into ours.
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        let child = cmd
            .spawn()
            .map_err(|e| format!("couldn't start Studio ({}): {e}", exe.display()))?;
        let pgid = child.id() as i32;
        Ok(Studio {
            child: Mutex::new(child),
            pgid,
            status: Mutex::new(None),
        })
    }

    /// Wait up to `ms` for the main process to exit. Returns true if it has.
    pub fn wait(&self, ms: u32) -> bool {
        if self.status.lock().expect("status").is_some() {
            return true;
        }
        let deadline = Instant::now() + Duration::from_millis(u64::from(ms));
        loop {
            match self.child.lock().expect("child").try_wait() {
                Ok(Some(status)) => {
                    *self.status.lock().expect("status") = Some(status);
                    return true;
                }
                Ok(None) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return false;
                    }
                    std::thread::sleep((deadline - now).min(Duration::from_millis(50)));
                }
                Err(e) => {
                    eprintln!("studio-run: waiting on Studio failed: {e}");
                    return false;
                }
            }
        }
    }

    /// The main process's exit code, once it has exited.
    pub fn exit_code(&self) -> Option<u32> {
        self.status
            .lock()
            .expect("status")
            .and_then(|s| s.code().map(|c| c as u32))
    }

    /// Kill Studio and every process still in its group.
    pub fn kill(&self) {
        if self.status.lock().expect("status").is_some() {
            return;
        }
        signal_group(self.pgid, SIGKILL);
        let _ = self.wait(5000);
    }

    /// macOS has no window list here. `--background` is what keeps Studio out
    /// of the foreground; a stuck login still shows up in Studio's log.
    pub fn windows(&self) -> Vec<WindowInfo> {
        Vec::new()
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        self.kill();
    }
}

fn signal_group(pgid: i32, sig: i32) {
    unsafe { kill(-pgid, sig) };
}

unsafe extern "C" {
    #[link_name = "kill"]
    fn kill(pid: i32, sig: i32) -> i32;
}
