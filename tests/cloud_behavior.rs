//! Checks that Open Cloud Luau Execution tasks still behave the way the
//! `--cloud` host relies on.
//!
//! Each test runs a probe from tests/cloud_probes as a real task in your test
//! experience and checks findings from docs/cloud-behavior.md. When one fails,
//! Roblox has changed: update the doc, and studio-run or docs/design.md if needed.
//! Needs the setup in docs/cloud-testing.md; ignored by default.
//!
//!     cargo test --test cloud_behavior -- --ignored --nocapture --test-threads=1
//!
//! Task creation is rate limited (5 per minute per key owner), so the client
//! paces itself and a full run takes a few minutes. With --nocapture, each test
//! prints what it saw as `FINDING` lines.

mod common;

use std::time::{Duration, Instant};

use common::cloud::{
    TaskOptions, binary_output, cloud_env, create_binary_input, create_task, get_task, is_done,
    request, run_task, task_logs, task_url, upload_place, upload_place_bytes, wait_task,
};
use common::{fixture, root};
use serde_json::{Value, json};

fn probe(name: &str) -> String {
    std::fs::read_to_string(root().join("tests/cloud_probes").join(name)).unwrap()
}

/// Prints a finding, for reading with --nocapture.
fn report(name: &str, value: &Value) {
    println!(
        "FINDING {name}: {}",
        serde_json::to_string_pretty(value).unwrap()
    );
}

/// The parts of a finished task worth reporting (not the script).
fn summary(task: &Value) -> Value {
    json!({
        "state": task["state"],
        "error": task["error"],
        "output": task["output"],
        "path": task["path"],
        "createTime": task["createTime"],
        "updateTime": task["updateTime"],
    })
}

/// The task's first return value.
fn result(task: &Value) -> &Value {
    assert_eq!(task["state"], "COMPLETE", "{}", summary(task));
    &task["output"]["results"][0]
}

/// All structured log messages of a task, flattened across pages.
fn messages(task: &Value) -> Vec<Value> {
    task_logs(task, "STRUCTURED")
        .iter()
        .flat_map(|page| {
            page["structuredMessages"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

fn is_error_string(v: &Value, containing: &str) -> bool {
    v.as_str()
        .is_some_and(|s| s.starts_with("error: ") && s.contains(containing))
}

/// The environment a task runs in (cloud-behavior.md, "Environment").
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn environment() {
    let task = run_task(&probe("environment.luau"), &TaskOptions::default());
    report("environment", &summary(&task));
    let r = result(&task);

    assert!(
        is_error_string(&r["loadstring"], "loadstring() is not available"),
        "loadstring: {}",
        r["loadstring"]
    );
    assert_eq!(r["setfenv"], true);
    assert_eq!(r["messageOut"], true);
    assert_eq!(r["scriptContextError"], true);
    assert_eq!(r["requireNewModule"], 42);
    assert_eq!(r["debugInfoSource"], "TaskScript");
    assert!(r["traceback"].as_str().unwrap().starts_with("TaskScript:"));
    assert_eq!(
        r["runService"],
        json!({ "isServer": true, "isClient": false, "isStudio": false, "isEdit": false,
                "isRunMode": false, "isRunning": false })
    );
    assert_eq!(r["pluginGlobal"], "nil");
    assert!(
        is_error_string(&r["changeHistory"], "lacking capability Plugin"),
        "changeHistory: {}",
        r["changeHistory"]
    );
    assert_eq!(r["scriptGlobal"], "nil");
    assert_eq!(r["varargCount"], 0, "no binary input: no arguments");
    assert_eq!(r["taskWait"], true);
    assert_eq!(r["placeId"], cloud_env().place_id);
    assert_eq!(r["gameId"], cloud_env().universe_id);
    // HttpService follows the experience's "Allow HTTP Requests" setting, so
    // it isn't asserted; the finding records what the test experience has.
}

/// How return values come back in output.results.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn return_values() {
    let task = run_task(&probe("return_values.luau"), &TaskOptions::default());
    report("return_values", &summary(&task));
    assert_eq!(task["state"], "COMPLETE");
    assert_eq!(
        task["output"]["results"],
        json!([
            1, 2.5, "two", true, null, [1, 2], { "a": 1 }, null,
            { "t": "buffer", "base64": "AAA=", "m": null },
            { "t": "numeric", "v": "nan", "m": null },
        ])
    );
}

/// A task script that doesn't compile fails the task, with no logs.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn compile_error() {
    let task = run_task(&probe("compile_error.luau"), &TaskOptions::default());
    report("compile_error", &summary(&task));
    assert_eq!(task["state"], "FAILED");
    assert_eq!(task["error"]["code"], "SCRIPT_ERROR");
    assert_eq!(
        task["error"]["message"],
        "TaskScript:3: Expected identifier when parsing expression, got '='"
    );
    assert!(messages(&task).is_empty());
}

/// An uncaught error fails the task; the message has no traceback, the logs do.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn runtime_error() {
    let task = run_task(&probe("runtime_error.luau"), &TaskOptions::default());
    report("runtime_error", &summary(&task));
    assert_eq!(task["state"], "FAILED");
    assert_eq!(task["error"]["code"], "SCRIPT_ERROR");
    assert_eq!(
        task["error"]["message"],
        "TaskScript:4: probe runtime error"
    );
    let logs = messages(&task);
    report("runtime_error logs", &json!(logs));
    assert_eq!(logs[0]["message"], "before the error");
    assert_eq!(logs[1]["messageType"], "ERROR");
    assert!(
        logs[1]["message"]
            .as_str()
            .unwrap()
            .contains("Script 'TaskScript', Line 4")
    );
}

/// The user's source compiled as a ModuleScript: the cloud wrapper's approach (docs/design.md).
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn module_loading() {
    let task = run_task(&probe("module_loading.luau"), &TaskOptions::default());
    report("module_loading", &summary(&task));
    let r = result(&task);

    // A compile error: require only says it failed; the message arrives through MessageOut.
    assert_eq!(r["compileError"]["ok"], false);
    assert_eq!(
        r["compileError"]["err"],
        "Requested module experienced an error while loading"
    );
    let compile_msg =
        "MessageError: tests/compile.luau:2: Expected identifier when parsing expression, got '='";
    assert!(
        r["logged"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == compile_msg),
        "logged: {}",
        r["logged"]
    );

    // Errors and tracebacks use the module's name and the user's own line numbers.
    assert_eq!(r["runtimeError"]["err"], "tests/runtime.luau:3: boom xy");
    let tb = r["runtimeError"]["traceback"].as_str().unwrap();
    assert!(tb.starts_with("tests/runtime.luau:3\n"), "traceback: {tb}");
    assert!(
        tb.lines().skip(1).all(|l| l.starts_with("TaskScript:")),
        "the rest is the wrapper's frames: {tb}"
    );

    // print can be replaced with setfenv, and return values come back.
    assert_eq!(
        r["capture"],
        json!({ "loaded": true, "returned": 7, "captured": ["hello"] })
    );

    // Errors in spawned threads name the module in ScriptContext.Error's stack.
    assert_eq!(
        r["contextErrors"],
        json!([{ "message": "tests/async.luau:2: async boom",
                 "stack": "tests/async.luau, line 2\n" }])
    );

    // A module run verbatim must return exactly one value.
    let verbatim = &r["verbatim"];
    assert_eq!(
        verbatim["none"]["v"],
        "Module code did not return exactly one value"
    );
    assert_eq!(verbatim["one"]["v"], "5");
    assert_eq!(verbatim["exportType"]["ok"], true);
}

/// Timeouts: the maximum, and what a timed-out task leaves behind.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn timeouts() {
    let env = cloud_env();
    let url = format!(
        "/cloud/v2/universes/{}/places/{}/luau-execution-session-tasks",
        env.universe_id, env.place_id
    );
    let r = request(
        "POST",
        &url,
        Some(&json!({ "script": "return 1", "timeout": "301s" })),
    );
    report(
        "timeout 301s",
        &json!({ "status": r.status, "body": r.body }),
    );
    assert_eq!(r.status, 400);
    assert_eq!(r.body["code"], "INVALID_ARGUMENT");

    let task = run_task(
        &probe("slow.luau"),
        &TaskOptions {
            timeout: Some(5),
            ..Default::default()
        },
    );
    report("timeout 5s", &summary(&task));
    assert_eq!(task["state"], "FAILED");
    assert_eq!(task["error"]["code"], "DEADLINE_EXCEEDED");
    // Output from before the timeout is kept.
    let logs = messages(&task);
    assert_eq!(logs[0]["message"], "PROBE-early");
    assert_eq!(logs[1]["messageType"], "WARNING");
}

/// Logs are empty until the task finishes, and there's no way to cancel a task.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn logs_while_running_and_cancel() {
    let task = create_task(
        &probe("slow.luau"),
        &TaskOptions {
            timeout: Some(120),
            ..Default::default()
        },
    );
    let start = Instant::now();
    let mut t = task.clone();
    while t["state"] != "PROCESSING" && !is_done(&t) {
        std::thread::sleep(Duration::from_secs(2));
        t = get_task(&task);
    }
    assert_eq!(t["state"], "PROCESSING");

    // The early prints happen within a second; logs still read empty mid-run.
    std::thread::sleep(Duration::from_secs(8));
    let mid = task_logs(&task, "STRUCTURED");
    report("logs while running", &json!(mid));
    assert!(
        mid.iter()
            .all(|p| p["structuredMessages"].as_array().is_none_or(Vec::is_empty)),
        "logs while running: {mid:?}"
    );

    // None of the plausible ways to cancel exists.
    let path = task_url(&task);
    let attempts = [
        ("PATCH", path.clone(), Some(json!({ "state": "CANCELLED" }))),
        (
            "PATCH",
            format!("{path}?updateMask=state"),
            Some(json!({ "state": "CANCELLED" })),
        ),
        ("POST", format!("{path}:cancel"), Some(json!({}))),
        ("DELETE", path.clone(), None),
    ];
    for (method, url, body) in attempts {
        let r = request(method, &url, body.as_ref());
        report(
            "cancel attempt",
            &json!({ "method": method, "url": url, "status": r.status, "body": r.body }),
        );
        assert_eq!(
            r.status, 404,
            "{method} {url} now works: cancelling may exist"
        );
    }

    let t = wait_task(&task, Duration::from_secs(240));
    report("slow final", &summary(&t));
    assert_eq!(t["state"], "COMPLETE", "the task ran to the end");
    assert!(start.elapsed() >= Duration::from_secs(35));
    let logs = messages(&t);
    let texts: Vec<_> = logs.iter().map(|m| m["message"].clone()).collect();
    assert_eq!(texts, ["PROBE-early", "PROBE-early-warn", "PROBE-late"]);
}

/// Binary input arrives as the task's only argument; binary output round-trips.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn binary_input_and_output() {
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let (input, method) = create_binary_input(&data);
    report(
        "binary input",
        &json!({ "path": input, "uploadMethod": method }),
    );
    assert_eq!(method, "PUT");
    let task = run_task(
        &probe("binary_io.luau"),
        &TaskOptions {
            binary_input: Some(input),
            enable_binary_output: true,
            ..Default::default()
        },
    );
    report("binary task", &summary(&task));
    assert_eq!(task["state"], "COMPLETE");
    assert_eq!(
        task["output"]["results"],
        json!([1000, "table", ["BinaryInput"]])
    );
    let out = binary_output(&task);
    let mut reversed = data.clone();
    reversed.reverse();
    assert_eq!(out, reversed);
}

/// A version uploaded as Saved (unpublished) can run tasks.
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn saved_version() {
    // The legacy upload rejects XML with a trailing newline (the fixture has one).
    let raw = std::fs::read(fixture("marker.rbxlx")).unwrap();
    assert!(raw.ends_with(b"\n"));
    let r = upload_place_bytes(&raw, "application/xml", "Saved");
    report(
        "upload with trailing newline",
        &json!({ "status": r.status, "body": r.body }),
    );
    assert_eq!(r.status, 400);
    assert_eq!(r.body["message"], "Invalid Content stream");

    // Trimmed, it uploads.
    let r = upload_place(&fixture("marker.rbxlx"), "Saved");
    report(
        "upload Saved",
        &json!({ "status": r.status, "body": r.body }),
    );
    assert!(
        r.ok(),
        "upload failed: HTTP {} {} (the key needs Places write; see docs/cloud-testing.md)",
        r.status,
        r.body
    );
    let version = r.body["versionNumber"].as_u64().expect("versionNumber");

    let on_version = run_task(
        &probe("marker_place.luau"),
        &TaskOptions {
            version: Some(version),
            ..Default::default()
        },
    );
    report("task on the Saved version", &summary(&on_version));
    assert_eq!(
        *result(&on_version),
        json!({ "placeVersion": version, "marker": true, "baseplate": false })
    );

    // What "latest" means once a Saved version exists.
    let latest = run_task(&probe("marker_place.luau"), &TaskOptions::default());
    report("latest after upload", &summary(&latest));
    // A task without a version runs on the latest version, Saved or not.
    assert_eq!(result(&latest)["placeVersion"], version);
}

/// A place's own ModuleScripts can be loaded the way --trace-modules does it
/// under --cloud (docs/design.md).
#[test]
#[ignore = "needs an Open Cloud test experience; see docs/cloud-testing.md"]
fn place_modules() {
    let r = upload_place(&fixture("modules.rbxlx"), "Saved");
    assert!(r.ok(), "upload failed: HTTP {} {}", r.status, r.body);
    let version = r.body["versionNumber"].as_u64().expect("versionNumber");
    let task = run_task(
        &probe("place_modules.luau"),
        &TaskOptions {
            version: Some(version),
            ..Default::default()
        },
    );
    report("place_modules", &summary(&task));
    let r = result(&task);

    // A task can read the Source of a module that's part of the place.
    assert_eq!(r["found"], true);
    assert_eq!(r["readSource"]["ok"], true);
    assert!(r["readSource"]["length"].as_u64().unwrap() > 0);

    // Loaded under its full name, all its output is captured, top level included.
    assert_eq!(r["fullName"], "ReplicatedStorage.Greeter");
    assert_eq!(
        r["load"],
        json!({ "ok": true, "returnedTable": true,
                "captured": ["Greeter top-level print", "Greeter.greet says hi to probe"] })
    );

    // Errors and tracebacks name it, with the module's own line numbers.
    assert_eq!(
        r["boom"]["err"],
        "ReplicatedStorage.Greeter:8: Greeter.boom error"
    );
    let tb = r["boom"]["traceback"].as_str().unwrap();
    assert!(
        tb.starts_with("ReplicatedStorage.Greeter:8 function boom\n"),
        "traceback: {tb}"
    );

    // A compile error in a module named with dots arrives through MessageOut, named.
    assert_eq!(
        r["compileError"]["err"],
        "Requested module experienced an error while loading"
    );
    let compile_msg = "MessageError: ReplicatedStorage.Bad:1: Expected identifier when parsing expression, got '='";
    assert!(
        r["logged"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == compile_msg),
        "logged: {}",
        r["logged"]
    );

    // Errors in threads it spawns name it in ScriptContext.Error's stack.
    assert_eq!(
        r["contextErrors"],
        json!([{ "message": "ReplicatedStorage.Async:2: async boom",
                 "stack": "ReplicatedStorage.Async, line 2\n" }])
    );
}
