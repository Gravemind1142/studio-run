//! Helpers shared by the tests that launch real Roblox Studio.
#![allow(dead_code)]

pub mod cloud;
pub mod mock;
pub mod structured_logs;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime};

/// At most this many Studio instances at once, across all tests in a binary.
const MAX_PARALLEL: usize = 3;
static SLOTS: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

/// Holds one of the Studio slots until dropped.
pub struct Slot;

impl Slot {
    pub fn take() -> Slot {
        let mut used = SLOTS.lock().unwrap();
        while *used >= MAX_PARALLEL {
            used = FREED.wait(used).unwrap();
        }
        *used += 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *SLOTS.lock().unwrap() -= 1;
        FREED.notify_one();
    }
}

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

pub fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_studio-run")
}

/// Runs studio-run with `args`, holding a Studio slot while it runs.
pub fn studio_run(args: &[&str]) -> Output {
    let _slot = Slot::take();
    let out = Command::new(exe())
        .args(args)
        .output()
        .expect("spawn studio-run");
    let o = Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    eprintln!(
        "--- studio-run {args:?} -> {}\n{}\n{}",
        o.code,
        truncate(&o.stdout, 4000),
        o.stderr
    );
    o
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// A fresh directory for one test's files. Not removed afterwards, so the
/// files can be inspected when a check fails.
pub fn temp_dir(test: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir()
        .join("studio-run-tests")
        .join(format!("{test}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Studio's log for the run whose command line contains `marker`, if it has
/// started one since `since`.
pub fn find_studio_log(marker: &str, since: SystemTime) -> Option<PathBuf> {
    let dir = logs_dir()?;
    let since = since - Duration::from_secs(5);
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.contains("_Studio_") && !name.contains("CrashHandler") && name.ends_with(".log")
        })
        .filter(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t >= since)
        })
        .find(|p| header(p).contains(marker))
}

fn logs_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Roblox/logs"))
    }
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Logs/Roblox"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

fn header(path: &Path) -> String {
    use std::io::Read;
    let mut buf = vec![0; 16 * 1024];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut buf))
        .unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// Reads a file that Studio may still be writing, lossily.
pub fn read_lossy(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Input files for `tests/fixtures/binary_io.luau`, in a fresh temp directory:
/// `a` holds every byte value and the event sentinel, `b` is `b_len` bytes of
/// noise. Returns the `--input` arguments, an `--binary-output` path, and the
/// binary output the script should produce.
pub fn binary_io_case(tag: &str, b_len: usize) -> (Vec<String>, PathBuf, Vec<u8>) {
    let dir = std::env::temp_dir().join(format!(
        "studio-run-test-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut a: Vec<u8> = (0..=255).collect();
    a.extend_from_slice(b"@@studio-run@@{\"v\":1,\"seq\":1,\"kind\":\"result\"}");
    let mut b = Vec::with_capacity(b_len);
    let mut x: u32 = 0x1234_5678;
    for _ in 0..b_len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        b.push(x as u8);
    }
    std::fs::write(dir.join("a.bin"), &a).unwrap();
    std::fs::write(dir.join("b.bin"), &b).unwrap();
    let args = vec![
        format!("a={}", dir.join("a.bin").display()),
        format!("b={}", dir.join("b.bin").display()),
    ];
    let expected: Vec<u8> = a.iter().rev().chain(b.iter().rev()).copied().collect();
    (args, dir.join("out.bin"), expected)
}
