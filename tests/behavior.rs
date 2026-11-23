//! Checks that Roblox Studio still behaves the way studio-run relies on.
//!
//! Each test checks one finding from docs/internals/studio-behavior.md. When one fails
//! after a Studio update, Studio has changed: update the doc, and studio-run if
//! needed. Needs Studio installed and logged in; ignored by default.
//!
//!     cargo test --test behavior -- --ignored --nocapture
//!
//! Tests named `raw_*` launch Studio without the wrapper (`studio-run raw`), to
//! see Studio's own behaviour. The others run a probe from tests/probes
//! through `studio-run run --no-capture` and read the JSON it reports.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

use common::{Slot, find_studio_log, fixture, read_lossy, root, studio_run, temp_dir};
use serde_json::Value;

const EXIT_TIMEOUT: i32 = 124;

fn probe_path(name: &str) -> PathBuf {
    root().join("tests/probes").join(name)
}

/// Runs a probe through the wrapper and returns the JSON it reported.
fn probe(name: &str, extra: &[&str]) -> Value {
    let path = probe_path(name);
    let mut args = vec![
        "run",
        path.to_str().unwrap(),
        "--no-capture",
        "--format",
        "jsonl",
        "--timeout",
        "120",
    ];
    args.extend_from_slice(extra);
    let o = studio_run(&args);
    let report = o
        .stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|e| {
            let m = e["message"].as_str()?;
            m.strip_prefix("@@probe@@")
                .map(|json| serde_json::from_str(json).expect("probe JSON"))
        });
    report.unwrap_or_else(|| panic!("{name} reported nothing (exit code {})", o.code))
}

/// One unwrapped Studio run.
struct Raw {
    code: i32,
    /// Contents of `--outputFile`, if Studio wrote it.
    output: Option<String>,
    /// Studio's own log for this run.
    log: Option<String>,
}

impl Raw {
    fn exited_cleanly(&self) -> bool {
        self.code == 0
    }
    fn output(&self) -> &str {
        self.output.as_deref().unwrap_or("")
    }
    fn log(&self) -> &str {
        self.log.as_deref().unwrap_or("")
    }
    fn outcome(&self) -> String {
        match self.code {
            0 => "exited 0".into(),
            EXIT_TIMEOUT => "didn't quit".into(),
            c => format!("exited {:#x}", c as u32),
        }
    }
}

fn raw_args<'a>(script: &'a str, output: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "raw",
        "--timeout",
        "60",
        "--",
        "--task",
        "RunScript",
        "--runScriptFile",
        script,
        "--outputFile",
        output,
        "--quitAfterExecution",
    ];
    args.extend_from_slice(extra);
    args
}

/// Runs `script` in Studio without the wrapper, with `--quitAfterExecution`.
fn raw(test: &str, script: &Path, extra: &[&str]) -> Raw {
    let dir = temp_dir(test);
    let output = dir.join("output.log");
    let started = SystemTime::now();
    let o = studio_run(&raw_args(
        script.to_str().unwrap(),
        output.to_str().unwrap(),
        extra,
    ));
    Raw {
        code: o.code,
        output: read_lossy(&output),
        log: find_studio_log(output.to_str().unwrap(), started).and_then(|p| read_lossy(&p)),
    }
}

/// Like `raw`, but tries up to three times to get a run where Studio quits
/// cleanly, since `--quitAfterExecution` doesn't always work.
fn raw_until_clean_exit(test: &str, script: &Path, extra: &[&str]) -> Raw {
    let mut outcomes = vec![];
    for _ in 0..3 {
        let r = raw(test, script, extra);
        if r.exited_cleanly() {
            return r;
        }
        outcomes.push(r.outcome());
    }
    panic!("Studio never quit cleanly in 3 attempts: {outcomes:?}");
}

// ---------------------------------------------------------------- errors

fn errors_run() -> &'static Raw {
    static RUN: OnceLock<Raw> = OnceLock::new();
    RUN.get_or_init(|| raw_until_clean_exit("errors", &probe_path("raw_errors.luau"), &[]))
}

#[test]
#[ignore]
fn raw_uncaught_error_still_exits_0() {
    assert_eq!(errors_run().code, 0);
}

#[test]
#[ignore]
fn raw_uncaught_error_before_a_yield_is_reported() {
    let out = errors_run().output();
    assert!(out.contains("RunScript:11: PROBE-uncaught"), "{out}");
}

#[test]
#[ignore]
fn raw_uncaught_error_after_a_yield_is_lost() {
    let r = raw_until_clean_exit("yield", &probe_path("raw_error_after_yield.luau"), &[]);
    assert!(r.output().contains("PROBE-before"), "script didn't run");
    assert!(
        !r.output().contains("PROBE-uncaught") && !r.log().contains("PROBE-uncaught"),
        "Studio now reports uncaught errors after a yield"
    );
}

#[test]
#[ignore]
fn raw_spawned_thread_errors_are_reported_as_runscript() {
    let out = errors_run().output();
    assert!(out.contains("RunScript:5: PROBE-spawned-error"), "{out}");
    assert!(out.contains("Script 'RunScript', Line 5"), "{out}");
}

#[test]
#[ignore]
fn raw_spawned_threads_outlive_the_chunk() {
    assert!(errors_run().output().contains("PROBE-delayed"));
}

// ---------------------------------------------------------------- output

struct OutputRun {
    raw: Raw,
    /// The live log showed the first print before Studio exited.
    log_was_live: bool,
    /// The output file's contents at that moment.
    output_mid_run: Option<String>,
}

/// Runs raw_output.luau, and while it pauses, compares the live log with the
/// output file.
fn output_run() -> &'static OutputRun {
    static RUN: OnceLock<OutputRun> = OnceLock::new();
    RUN.get_or_init(|| {
        let _slot = Slot::take();
        let dir = temp_dir("output");
        let output = dir.join("output.log");
        let output_str = output.to_str().unwrap();
        let script = probe_path("raw_output.luau");
        let started = SystemTime::now();
        let mut child = Command::new(common::exe())
            .args(raw_args(script.to_str().unwrap(), output_str, &[]))
            .spawn()
            .expect("spawn studio-run raw");

        let mut log_was_live = false;
        let mut output_mid_run = None;
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            let log = find_studio_log(output_str, started).and_then(|p| read_lossy(&p));
            if log.is_some_and(|l| l.contains("PROBE-early")) {
                log_was_live = true;
                output_mid_run = read_lossy(&output);
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let code = child.wait().unwrap().code().unwrap_or(-1);
        OutputRun {
            raw: Raw {
                code,
                output: read_lossy(&output),
                log: find_studio_log(output_str, started).and_then(|p| read_lossy(&p)),
            },
            log_was_live,
            output_mid_run,
        }
    })
}

#[test]
#[ignore]
fn raw_log_is_written_live() {
    assert!(output_run().log_was_live);
}

#[test]
#[ignore]
fn raw_output_file_is_written_only_at_the_end() {
    let r = output_run();
    assert!(
        r.log_was_live,
        "couldn't check: the log never showed output"
    );
    assert!(
        !r.output_mid_run
            .as_deref()
            .unwrap_or("")
            .contains("PROBE-early"),
        "short output now reaches the output file while the script runs"
    );
    assert!(r.raw.output().contains("PROBE-late"), "{}", r.raw.outcome());
}

#[test]
#[ignore]
fn raw_log_lines_are_cut_near_1000_chars() {
    let log = output_run().raw.log();
    let line = log
        .lines()
        .find(|l| l.contains("PROBE-LONG:"))
        .expect("long line in log");
    let xs = line.split("PROBE-LONG:").nth(1).unwrap().len();
    eprintln!("log line: {} chars, {xs} of 5000 x's kept", line.len());
    assert!(xs < 5000, "Studio's log no longer cuts long lines");
    assert!(xs > 500, "Studio's log cuts lines much shorter than before");
}

#[test]
#[ignore]
fn raw_output_file_keeps_long_lines() {
    let out = output_run().raw.output();
    assert!(out.contains(&format!("PROBE-LONG:{}", "x".repeat(5000))));
}

#[test]
#[ignore]
fn raw_source_is_echoed() {
    let out = output_run().raw.output();
    assert!(out.contains("> -- Run without the wrapper"), "{out}");
    // The echo is the only place the unsplit strings appear.
    assert!(out.contains("print(\"PROBE\" .. \"-early\")"), "{out}");
}

#[test]
#[ignore]
fn raw_saved_login_is_reused() {
    let log = output_run().raw.log();
    assert!(log.contains("Authenticated : YES"), "not logged in?");
}

#[test]
#[ignore]
fn raw_output_file_needs_log_extension() {
    let dir = temp_dir("extension");
    let output = dir.join("output.txt");
    let script = probe_path("raw_quit.luau");
    let started = SystemTime::now();
    studio_run(&raw_args(
        script.to_str().unwrap(),
        output.to_str().unwrap(),
        &[],
    ));
    let log = find_studio_log(output.to_str().unwrap(), started)
        .and_then(|p| read_lossy(&p))
        .unwrap_or_default();
    assert!(!output.exists(), "Studio now writes non-.log output files");
    assert!(log.contains("Output file must have a .log extension"));
}

// ---------------------------------------------------------------- scripts

#[test]
#[ignore]
fn raw_bom_makes_studio_run_nothing() {
    let dir = temp_dir("bom");
    let source = "print(\"PROBE\" .. \"-ran\")\n";
    let plain = dir.join("plain.luau");
    let bom = dir.join("bom.luau");
    std::fs::write(&plain, source).unwrap();
    std::fs::write(
        &bom,
        [b"\xEF\xBB\xBF".as_slice(), source.as_bytes()].concat(),
    )
    .unwrap();

    let plain = raw("bom", &plain, &[]);
    let bom = raw("bom", &bom, &[]);
    assert!(plain.output().contains("PROBE-ran"), "control run failed");
    assert!(
        !bom.output().contains("PROBE-ran") && !bom.log().contains("PROBE-ran"),
        "Studio now runs scripts that start with a BOM"
    );
}

#[test]
#[ignore]
fn raw_local_place_file_is_not_saved() {
    let dir = temp_dir("place");
    let place = dir.join("marker.rbxlx");
    std::fs::copy(fixture("marker.rbxlx"), &place).unwrap();
    let before = std::fs::read(&place).unwrap();

    let r = raw(
        "place",
        &probe_path("raw_place.luau"),
        &["--localPlaceFile", place.to_str().unwrap()],
    );
    assert!(
        r.output().contains("PROBE-marker-found:true"),
        "place not loaded"
    );
    assert!(r.output().contains("PROBE-changed"));
    assert_eq!(
        std::fs::read(&place).unwrap(),
        before,
        "place file was saved"
    );
}

/// Reports how often `--quitAfterExecution` actually makes Studio exit. This
/// is a measurement, not a pass/fail check: read its output with --nocapture.
#[test]
#[ignore]
fn raw_quit_after_execution_reliability() {
    let outcomes: Vec<String> = (0..3)
        .map(|_| raw("quit", &probe_path("raw_quit.luau"), &[]).outcome())
        .collect();
    let clean = outcomes.iter().filter(|o| *o == "exited 0").count();
    eprintln!("--quitAfterExecution: {clean} of 3 runs exited cleanly: {outcomes:?}");
}

// ---------------------------------------------------------------- probes

#[test]
#[ignore]
fn script_capabilities() {
    let r = probe("capabilities.luau", &[]);
    for available in [
        "loadstring",
        "setfenv",
        "debugInfo",
        "debugTraceback",
        "messageOut",
        "scriptContextError",
        "readModuleSource",
    ] {
        assert_eq!(
            r[available], true,
            "{available} is no longer available: {r}"
        );
    }
    for unavailable in ["processService", "debugLoadmodule", "fileSystemService"] {
        assert_eq!(r[unavailable], false, "{unavailable} is now available: {r}");
    }
}

#[test]
#[ignore]
fn message_out_cuts_near_100k_chars() {
    let r = probe("message_size.luau", &[]);
    eprintln!("MessageOut sizes: {r}");
    let seen = |sent: u64| {
        r.as_array()
            .unwrap()
            .iter()
            .find(|e| e["sent"] == sent)
            .and_then(|e| e["seen"].as_u64())
            .unwrap()
    };
    assert_eq!(seen(1000), 1000);
    assert_eq!(seen(10000), 10000);
    let big = seen(1_000_000);
    assert!(big < 1_000_000, "MessageOut no longer cuts long messages");
    assert!(
        big >= 50_000,
        "MessageOut cuts much shorter than before: {big}"
    );
}

#[test]
#[ignore]
fn spawned_thread_errors_reach_script_context() {
    let r = probe("script_errors.luau", &[]);
    let reported = r["reported"].as_array().unwrap();
    assert_eq!(reported.len(), 1, "{r}");
    let e = &reported[0];
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("probe_chunk.luau:2: spawned boom"),
        "{e}"
    );
    assert!(
        e["stack"]
            .as_str()
            .unwrap()
            .contains("probe_chunk.luau, line 2"),
        "{e}"
    );
    assert_eq!(e["originIsNil"], true, "{e}");
}

#[test]
#[ignore]
fn module_environments() {
    let place = fixture("modules.rbxlx");
    let r = probe("module_env.luau", &["--place", place.to_str().unwrap()]);
    eprintln!("module_env: {r}");
    assert_eq!(r["sharedGlobalsAffectModules"], false, "{r}");
    assert_eq!(r["threadEnvAffectsModules"], false, "{r}");
    assert_eq!(r["patchAfterRequireWorks"], true, "{r}");
    assert_eq!(r["ownLoaderCapturesTopLevel"], true, "{r}");
}

#[test]
#[ignore]
fn structured_logging() {
    let r = probe("structured_logs.luau", &[]);
    eprintln!("structured_logs: {r}");
    common::structured_logs::check(&r);
}
