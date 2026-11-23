//! End-to-end tests of `studio-run run --cloud` against real Open Cloud Luau
//! Execution tasks. They mirror tests/studio.rs, so the same scripts are
//! checked on both hosts.
//!
//! They need the test experience and key from docs/cloud-testing.md, and are
//! ignored by default:
//!
//!     cargo test --test cloud -- --ignored --test-threads=1
//!
//! Task creation is rate limited (5 per minute per key owner), so runs are
//! paced and the suite takes a few minutes.

mod common;

use std::process::Command;
use std::sync::Mutex;

use common::cloud::{API_KEY, cloud_env, pace_create};
use common::{Output, exe, fixture};
use serde_json::Value;

/// One cloud run at a time, so pacing works and output stays readable.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Runs studio-run with `args`, with the test experience and key.
/// `creates_task`: whether this run reaches the API (and so needs pacing).
fn cloud_run_with_key(args: &[&str], key: &str, creates_task: bool) -> Output {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let env = cloud_env();
    if creates_task {
        pace_create();
    }
    let ids = [
        "--cloud".to_string(),
        "--universe-id".into(),
        env.universe_id.to_string(),
        "--place-id".into(),
        env.place_id.to_string(),
        "--api-key-env".into(),
        API_KEY.into(),
    ];
    // The subcommand, then the cloud options, then the rest (which may end in `-- ...`).
    let out = Command::new(exe())
        .arg(args[0])
        .args(&ids)
        .args(&args[1..])
        .env(API_KEY, key)
        // Run outside the checkout, so a studio-run.toml there can't change the tests.
        .current_dir(std::env::temp_dir())
        .output()
        .expect("spawn studio-run");
    let o = Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    eprintln!(
        "--- studio-run {args:?} --cloud -> {}\n{}\n{}",
        o.code, o.stdout, o.stderr
    );
    o
}

fn cloud_run(args: &[&str]) -> Output {
    cloud_run_with_key(args, &cloud_env().api_key, true)
}

fn run_jsonl(script: &str, extra: &[&str]) -> Output {
    let path = fixture(script);
    let mut args = vec!["run", path.to_str().unwrap(), "--format", "jsonl"];
    args.extend_from_slice(extra);
    cloud_run(&args)
}

trait Events {
    fn events(&self) -> Vec<Value>;
    fn script_output(&self, level: &str) -> Vec<String>;
}

impl Events for Output {
    fn events(&self) -> Vec<Value> {
        self.stdout
            .lines()
            .map(|l| serde_json::from_str(l).expect("jsonl line"))
            .collect()
    }
    fn script_output(&self, level: &str) -> Vec<String> {
        self.events()
            .into_iter()
            .filter(|e| e["kind"] == "output" && e["source"] == "script" && e["level"] == level)
            .map(|e| e["message"].as_str().unwrap_or("").to_string())
            .collect()
    }
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn text_output_goes_to_stdout_and_stderr() {
    let path = fixture("print_warn.luau");
    let o = cloud_run(&["run", path.to_str().unwrap(), "--output", "script"]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stdout.trim(), "hello 1 true");
    assert!(o.stderr.contains("warning: careful"));
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn events_are_categorized_and_start_names_the_task() {
    let o = run_jsonl("print_warn.luau", &[]);
    assert_eq!(o.code, 0);
    assert_eq!(o.script_output("print"), ["hello 1 true"]);
    assert_eq!(o.script_output("warn"), ["careful"]);
    let ev = o.events();
    let start = &ev[0];
    assert_eq!(start["kind"], "start");
    assert_eq!(start["host"], "cloud");
    assert_eq!(start["chunk"], "print_warn.luau");
    assert_eq!(start["universeId"], cloud_env().universe_id);
    assert_eq!(start["placeId"], cloud_env().place_id);
    assert!(start["placeVersion"].is_u64());
    assert!(start["taskId"].is_string());
    let last = ev.last().unwrap();
    assert_eq!(last["kind"], "result");
    assert_eq!(last["ok"], true);
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn uncaught_error_fails_with_location() {
    let o = run_jsonl("error.luau", &[]);
    assert_eq!(o.code, 1);
    let errors = o.script_output("error");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("error.luau:2: boom"), "{errors:?}");
    // The wrapper's own frames are dropped from the traceback.
    assert!(!errors[0].contains("TaskScript"), "{errors:?}");
    assert_eq!(o.events().last().unwrap()["reason"], "error");
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn return_value_sets_exit_code() {
    assert_eq!(run_jsonl("exit_code.luau", &[]).code, 7);
    assert_eq!(run_jsonl("return_false.luau", &[]).code, 1);
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn compile_error_fails() {
    let o = run_jsonl("compile_error.luau", &[]);
    assert_eq!(o.code, 1);
    assert_eq!(o.events().last().unwrap()["reason"], "compile");
    let errors = o.script_output("error");
    assert!(errors[0].starts_with("compile_error.luau:"), "{errors:?}");
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn spawned_thread_errors_are_attributed_once() {
    let o = run_jsonl("async_error.luau", &[]);
    assert_eq!(o.code, 0);
    let ev = o.events();
    let async_errors: Vec<_> = ev.iter().filter(|e| e["async"] == true).collect();
    assert_eq!(async_errors.len(), 1, "{async_errors:?}");
    assert!(
        async_errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("async_error.luau:2: async boom")
    );
    assert!(
        !ev.iter().any(|e| e["source"] == "other"
            && e["message"].as_str().unwrap_or("").contains("async boom"))
    );
    assert_eq!(o.script_output("print"), ["main done"]);
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn long_messages_round_trip() {
    let o = run_jsonl("long_output.luau", &[]);
    let prints = o.script_output("print");
    assert_eq!(prints.len(), 1);
    assert_eq!(prints[0].len(), 15003);
    assert!(prints[0].ends_with("abcEND"));
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn unicode_and_bom() {
    assert_eq!(
        run_jsonl("unicode.luau", &[]).script_output("print"),
        ["héllo ✓"]
    );
    assert_eq!(
        run_jsonl("bom.luau", &[]).script_output("print"),
        ["bom ok"]
    );
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn no_capture_reports_prints_as_other() {
    let o = run_jsonl("print_warn.luau", &["--no-capture"]);
    assert_eq!(o.code, 0);
    assert!(o.script_output("print").is_empty());
    assert!(
        o.events()
            .iter()
            .any(|e| e["source"] == "other" && e["message"] == "hello 1 true")
    );
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn place_upload_with_real_require_then_place_version() {
    let place = fixture("modules.rbxlx");
    let o = run_jsonl(
        "modules.luau",
        &["--place", place.to_str().unwrap(), "--allow-upload"],
    );
    assert_eq!(o.code, 1); // Greeter.boom() errors
    assert_eq!(o.script_output("print"), ["hi tests"]);
    assert!(
        o.events()
            .iter()
            .any(|e| e["source"] == "other" && e["message"] == "Greeter.greet says hi to tests")
    );
    let version = o.events()[0]["placeVersion"].as_u64().unwrap();

    // The same version again, without uploading.
    let v = version.to_string();
    let o = run_jsonl("modules.luau", &["--place-version", &v]);
    assert_eq!(o.code, 1);
    assert_eq!(o.events()[0]["placeVersion"], version);
    assert_eq!(o.script_output("print"), ["hi tests"]);
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn timeout_exits_124() {
    let o = run_jsonl("forever.luau", &["--timeout", "5"]);
    assert_eq!(o.code, 124);
    assert!(o.stderr.contains("timed out"));
}

#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn bad_key_is_an_auth_error() {
    let path = fixture("print_warn.luau");
    let o = cloud_run_with_key(
        &["run", path.to_str().unwrap(), "--format", "jsonl"],
        "not-a-key",
        false,
    );
    assert_eq!(o.code, 125);
    let ev = o.events();
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(ev[0]["kind"], "error");
    assert_eq!(ev[0]["code"], "auth");
}

/// Options that make no sense in the cloud fail before any request is made.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn usage_errors_exit_2() {
    let path = fixture("print_warn.luau");
    let script = path.to_str().unwrap();
    let place = fixture("marker.rbxlx");
    for args in [
        vec!["run", script, "--visible"],
        vec!["run", script, "--timeout", "301"],
        vec!["run", script, "--trace-modules"],
        vec!["run", script, "--place", place.to_str().unwrap()], // no --allow-upload
        vec!["run", script, "--", "--someStudioArg"],
    ] {
        let o = cloud_run_with_key(&args, "unused", false);
        assert_eq!(o.code, 2, "{args:?}: {}", o.stderr);
    }
}

/// Inputs go up packed in the task's binary input; the binary output comes
/// back after the events in the task's.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn inputs_and_binary_output_round_trip() {
    let (inputs, out, expected) = common::binary_io_case("cloud-io", 1024 * 1024);
    let mut args = vec![];
    for i in &inputs {
        args.extend(["--input", i.as_str()]);
    }
    args.extend(["--binary-output", out.to_str().unwrap()]);
    let o = run_jsonl("binary_io.luau", &args);
    assert_eq!(o.code, 0);
    assert_eq!(
        o.script_output("print"),
        [format!("a,b {}", expected.len())]
    );
    let got = std::fs::read(&out).unwrap();
    assert!(
        got == expected,
        "binary output differs ({} bytes)",
        got.len()
    );
}
