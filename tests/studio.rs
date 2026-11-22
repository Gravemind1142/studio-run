//! End-to-end tests against a real Roblox Studio install.
//!
//! They need Studio installed and logged in, take ~10s each, and are ignored
//! by default. Run them with: `cargo test --test studio -- --ignored`

mod common;

use common::{Output, fixture, studio_run};
use serde_json::Value;

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

fn run_jsonl(script: &str, extra: &[&str]) -> Output {
    let path = fixture(script);
    let mut args = vec!["run", path.to_str().unwrap(), "--format", "jsonl"];
    args.extend_from_slice(extra);
    studio_run(&args)
}

#[test]
#[ignore]
fn text_output_goes_to_stdout_and_stderr() {
    let path = fixture("print_warn.luau");
    let o = studio_run(&["run", path.to_str().unwrap(), "--output", "script"]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stdout.trim(), "hello 1 true");
    assert!(o.stderr.contains("warning: careful"));
}

#[test]
#[ignore]
fn events_are_categorized() {
    let o = run_jsonl("print_warn.luau", &[]);
    assert_eq!(o.code, 0);
    assert_eq!(o.script_output("print"), ["hello 1 true"]);
    assert_eq!(o.script_output("warn"), ["careful"]);
    let ev = o.events();
    assert_eq!(ev.first().unwrap()["kind"], "start");
    let last = ev.last().unwrap();
    assert_eq!(last["kind"], "result");
    assert_eq!(last["ok"], true);
    // The source echo Studio prints for RunScript is dropped.
    assert!(!o.stdout.contains("studio-run wrapper"));
}

#[test]
#[ignore]
fn uncaught_error_fails_with_location() {
    let o = run_jsonl("error.luau", &[]);
    assert_eq!(o.code, 1);
    let errors = o.script_output("error");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("error.luau:2: boom"), "{errors:?}");
    assert_eq!(o.events().last().unwrap()["reason"], "error");
}

#[test]
#[ignore]
fn return_value_sets_exit_code() {
    assert_eq!(run_jsonl("exit_code.luau", &[]).code, 7);
    assert_eq!(run_jsonl("return_false.luau", &[]).code, 1);
}

#[test]
#[ignore]
fn compile_error_fails() {
    let o = run_jsonl("compile_error.luau", &[]);
    assert_eq!(o.code, 1);
    assert_eq!(o.events().last().unwrap()["reason"], "compile");
    assert!(o.script_output("error")[0].contains("compile_error.luau"));
}

#[test]
#[ignore]
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
    // Not reported a second time as `other`.
    assert!(
        !ev.iter().any(|e| e["source"] == "other"
            && e["message"].as_str().unwrap_or("").contains("async boom"))
    );
    assert_eq!(o.script_output("print"), ["main done"]);
}

#[test]
#[ignore]
fn long_messages_survive_studio_line_limits() {
    let o = run_jsonl("long_output.luau", &[]);
    let prints = o.script_output("print");
    assert_eq!(prints.len(), 1);
    assert_eq!(prints[0].len(), 15003);
    assert!(prints[0].ends_with("abcEND"));
}

#[test]
#[ignore]
fn unicode_round_trips() {
    assert_eq!(
        run_jsonl("unicode.luau", &[]).script_output("print"),
        ["héllo ✓"]
    );
}

#[test]
#[ignore]
fn bom_is_ignored() {
    assert_eq!(
        run_jsonl("bom.luau", &[]).script_output("print"),
        ["bom ok"]
    );
}

#[test]
#[ignore]
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
#[ignore]
fn place_file_with_real_require() {
    let place = fixture("modules.rbxlx");
    let o = run_jsonl("modules.luau", &["--place", place.to_str().unwrap()]);
    assert_eq!(o.code, 1); // Greeter.boom() errors
    assert_eq!(o.script_output("print"), ["hi tests"]);
    // The module uses the real print, so its output is `other`.
    assert!(
        o.events()
            .iter()
            .any(|e| e["source"] == "other" && e["message"] == "Greeter.greet says hi to tests")
    );
}

#[test]
#[ignore]
fn trace_modules_attributes_module_output() {
    let place = fixture("modules.rbxlx");
    let o = run_jsonl(
        "modules.luau",
        &["--place", place.to_str().unwrap(), "--trace-modules"],
    );
    assert_eq!(o.code, 1);
    assert_eq!(
        o.script_output("print"),
        [
            "Greeter top-level print",
            "Greeter.greet says hi to tests",
            "hi tests"
        ]
    );
    assert!(
        o.script_output("error")[0].starts_with("ReplicatedStorage.Greeter:8: Greeter.boom error")
    );
}

#[test]
#[ignore]
fn timeout_kills_studio() {
    let o = run_jsonl("forever.luau", &["--timeout", "20"]);
    assert_eq!(o.code, 124);
    assert!(o.stderr.contains("timed out"));
}

#[test]
#[ignore]
fn api_dump_writes_json() {
    let out = std::env::temp_dir().join(format!("studio-run-test-api-{}.json", std::process::id()));
    let o = studio_run(&["api-dump", out.to_str().unwrap()]);
    assert_eq!(o.code, 0);
    let json: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    // The V2 schema uses lowercase keys.
    assert!(json["classes"].as_array().is_some_and(|c| !c.is_empty()));
    let _ = std::fs::remove_file(out);
}
