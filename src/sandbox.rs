//! Launching Studio on a private, never-shown Windows desktop inside a job
//! object, so it can't appear on the user's screen or outlive us.

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::JobObjects::*;
use windows::Win32::System::StationsAndDesktops::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, PWSTR};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A running Studio process tree. Dropping it kills the tree and closes the
/// desktop; so does our own process exiting (the job is `KILL_ON_JOB_CLOSE`).
pub struct Studio {
    job: HANDLE,
    process: HANDLE,
    desktop: Option<HDESK>,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub class: String,
    pub title: String,
}

impl Studio {
    /// Start `exe args...`. With `hidden`, Studio runs on its own desktop.
    pub fn launch(exe: &Path, args: &[String], hidden: bool) -> Result<Studio, String> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null())
                .map_err(|e| format!("CreateJobObjectW: {e}"))?;
            let mut studio = Studio {
                job,
                process: HANDLE::default(),
                desktop: None,
            };

            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|e| format!("SetInformationJobObject: {e}"))?;

            let desk_name = format!(
                "studio-run-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let mut desk_path = wide(&format!("WinSta0\\{desk_name}"));
            if hidden {
                let name = wide(&desk_name);
                let desk = CreateDesktopW(
                    PCWSTR(name.as_ptr()),
                    PCWSTR::null(),
                    None,
                    DESKTOP_CONTROL_FLAGS(0),
                    GENERIC_ALL.0,
                    None,
                )
                .map_err(|e| format!("CreateDesktopW: {e}"))?;
                studio.desktop = Some(desk);
            }

            // Null std handles: Studio's own stdout must not mix into ours.
            let mut si = STARTUPINFOW {
                cb: size_of::<STARTUPINFOW>() as u32,
                dwFlags: STARTF_USESTDHANDLES,
                ..Default::default()
            };
            if hidden {
                si.lpDesktop = PWSTR(desk_path.as_mut_ptr());
            }

            let mut cmdline = quote(&exe.to_string_lossy());
            for a in args {
                cmdline.push(' ');
                cmdline.push_str(&quote(a));
            }
            let mut cmdline = wide(&cmdline);
            let mut pi = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(cmdline.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_SUSPENDED,
                None,
                PCWSTR::null(),
                &si,
                &mut pi,
            )
            .map_err(|e| format!("couldn't start Studio ({}): {e}", exe.display()))?;
            studio.process = pi.hProcess;

            // Assign before it runs, so every child process lands in the job too.
            let assigned = AssignProcessToJobObject(job, pi.hProcess);
            if let Err(e) = assigned {
                let _ = TerminateProcess(pi.hProcess, 1);
                let _ = CloseHandle(pi.hThread);
                return Err(format!("AssignProcessToJobObject: {e}"));
            }
            ResumeThread(pi.hThread);
            let _ = CloseHandle(pi.hThread);
            Ok(studio)
        }
    }

    /// Wait up to `ms` for the main process to exit. Returns true if it has.
    pub fn wait(&self, ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.process, ms) == WAIT_OBJECT_0 }
    }

    /// The main process's exit code, once it has exited.
    pub fn exit_code(&self) -> Option<u32> {
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(self.process, &mut code).ok()? };
        (code != STILL_ACTIVE.0 as u32).then_some(code)
    }

    /// Kill Studio and everything it started.
    pub fn kill(&self) {
        unsafe {
            let _ = TerminateJobObject(self.job, 1);
            WaitForSingleObject(self.process, 5000);
        }
    }

    /// Visible top-level windows belonging to the Studio process tree
    /// (useful to see what Studio is stuck on, e.g. a login dialog).
    pub fn windows(&self) -> Vec<WindowInfo> {
        let pids = self.pids();
        let mut hwnds: Vec<HWND> = Vec::new();
        let lp = LPARAM(&mut hwnds as *mut _ as isize);
        unsafe {
            match self.desktop {
                Some(d) => {
                    let _ = EnumDesktopWindows(Some(d), Some(collect), lp);
                }
                None => {
                    let _ = EnumWindows(Some(collect), lp);
                }
            }
        }
        hwnds
            .into_iter()
            .filter_map(|h| unsafe {
                let mut pid = 0u32;
                GetWindowThreadProcessId(h, Some(&mut pid));
                if !pids.contains(&pid) || !IsWindowVisible(h).as_bool() {
                    return None;
                }
                let mut t = [0u16; 256];
                let n = GetWindowTextW(h, &mut t);
                let mut c = [0u16; 256];
                let m = GetClassNameW(h, &mut c);
                Some(WindowInfo {
                    class: String::from_utf16_lossy(&c[..m as usize]),
                    title: String::from_utf16_lossy(&t[..n as usize]),
                })
            })
            .collect()
    }

    fn pids(&self) -> Vec<u32> {
        // JOBOBJECT_BASIC_PROCESS_ID_LIST: two u32 counts, then ULONG_PTR ids.
        let mut buf = vec![0usize; 1026];
        unsafe {
            if QueryInformationJobObject(
                Some(self.job),
                JobObjectBasicProcessIdList,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * size_of::<usize>()) as u32,
                None,
            )
            .is_err()
            {
                return vec![];
            }
        }
        let n = (buf[0] >> 32) as usize; // NumberOfProcessIdsInList
        buf[1..1 + n.min(buf.len() - 1)]
            .iter()
            .map(|&p| p as u32)
            .collect()
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        unsafe {
            let _ = TerminateJobObject(self.job, 1);
            if !self.process.is_invalid() {
                WaitForSingleObject(self.process, 5000);
                let _ = CloseHandle(self.process);
            }
            let _ = CloseHandle(self.job);
            if let Some(d) = self.desktop.take() {
                let _ = CloseDesktop(d);
            }
        }
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let v = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    v.push(hwnd);
    true.into()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Quote one argument for `CommandLineToArgvW`-style parsing.
pub fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quotes_arguments() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote(""), r#""""#);
        assert_eq!(quote(r"C:\My Place\x.rbxl"), r#""C:\My Place\x.rbxl""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        // Trailing backslashes are doubled so they don't escape the closing quote.
        assert_eq!(quote(r"C:\a b\"), r#""C:\a b\\""#);
    }
}
