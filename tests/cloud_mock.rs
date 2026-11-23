//! `studio-run run --cloud` against a local mock of Open Cloud: no network,
//! no key, no Roblox. These run with `cargo test`, so CI checks on every pull
//! request that the requests studio-run makes, and how it handles each kind of
//! response, haven't changed by accident.
//!
//! The mock's responses follow what the real API does (docs/internals/cloud-behavior.md).
//! The weekly cloud workflow runs tests/cloud.rs and tests/cloud_behavior.rs
//! against the real API, to catch Roblox changing it.

mod common;

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use common::mock::{MockApi, Request, Response};
use common::{Output, exe, fixture};
use serde_json::{Value, json};

const KEY: &str = "test-key-not-real";
const UNIVERSE: u64 = 1;
const PLACE: u64 = 2;
const CREATE_LATEST: &str = "/cloud/v2/universes/1/places/2/luau-execution-session-tasks";

fn task_path(version: u64) -> String {
    format!("universes/1/places/2/versions/{version}/luau-execution-sessions/s1/tasks/t1")
}

/// A line as the cloud wrapper writes it.
fn event(ev: Value) -> String {
    let mut ev = ev;
    ev["v"] = json!(1);
    format!("@@studio-run@@{ev}")
}

/// The events of a script that printed `hello` and returned `exit_code`.
fn events(exit_code: i64) -> Vec<String> {
    vec![
        event(json!({ "seq": 1, "kind": "start", "chunk": "print_warn.luau" })),
        event(
            json!({ "seq": 2, "kind": "output", "source": "script", "level": "print", "message": "hello" }),
        ),
        event(
            json!({ "seq": 3, "kind": "result", "ok": exit_code == 0, "exitCode": exit_code, "reason": "returned" }),
        ),
    ]
}

fn created(version: u64) -> Response {
    Response::json(
        200,
        json!({ "path": task_path(version), "state": "QUEUED" }),
    )
}

fn complete(base: &str, version: u64, exit_code: i64) -> Response {
    Response::json(
        200,
        json!({
            "path": task_path(version),
            "state": "COMPLETE",
            "binaryOutputUri": format!("{base}/download/t1"),
            "output": { "results": [{ "ok": exit_code == 0, "exitCode": exit_code, "reason": "returned" }] },
        }),
    )
}

/// Runs `studio-run run --cloud <args>` against the mock.
fn run(mock: &MockApi, args: &[&str]) -> Output {
    run_with_key(mock, args, Some(KEY))
}

fn run_with_key(mock: &MockApi, args: &[&str], key: Option<&str>) -> Output {
    let mut cmd = Command::new(exe());
    cmd.args(["run", "--cloud", "--poll-interval", "1"])
        .args([
            "--universe-id",
            &UNIVERSE.to_string(),
            "--place-id",
            &PLACE.to_string(),
        ])
        .args(args)
        .env("STUDIO_RUN_TEST_API_BASE", &mock.base)
        .env_remove("STUDIO_RUN_HOST")
        .current_dir(std::env::temp_dir());
    match key {
        Some(k) => cmd.env("ROBLOX_API_KEY", k),
        None => cmd.env_remove("ROBLOX_API_KEY"),
    };
    let out = cmd.output().expect("spawn studio-run");
    let o = Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    eprintln!(
        "--- studio-run run --cloud {args:?} -> {}\n{}\n{}",
        o.code, o.stdout, o.stderr
    );
    o
}

fn jsonl(o: &Output) -> Vec<Value> {
    o.stdout
        .lines()
        .map(|l| serde_json::from_str(l).expect("jsonl line"))
        .collect()
}

fn script(name: &str) -> String {
    fixture(name).to_str().unwrap().to_string()
}

/// A mock that serves a task finishing on `version` with `exit_code`.
/// `on_create` can refuse a create request (e.g. with 429) by returning a response.
fn happy_mock(
    version: u64,
    exit_code: i64,
    on_create: impl Fn(usize) -> Option<Response> + Send + Sync + 'static,
) -> MockApi {
    let base = Arc::new(OnceLock::<String>::new());
    let b = base.clone();
    let creates = AtomicUsize::new(0);
    let polls = AtomicUsize::new(0);
    let path = format!("/cloud/v2/{}", task_path(version));
    let mock = MockApi::start(move |r: &Request| {
        if r.method == "POST" && r.path().ends_with("/luau-execution-session-tasks") {
            let n = creates.fetch_add(1, Ordering::SeqCst);
            return on_create(n).unwrap_or_else(|| created(version));
        }
        if r.method == "POST" && r.path().starts_with("/universes/v1/") {
            return Response::json(200, json!({ "versionNumber": version }));
        }
        if r.method == "GET" && r.path() == path {
            // Still running on the first poll.
            return if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                Response::json(
                    200,
                    json!({ "path": task_path(version), "state": "PROCESSING" }),
                )
            } else {
                complete(b.get().unwrap(), version, exit_code)
            };
        }
        if r.method == "GET" && r.path() == "/download/t1" {
            return Response::bytes(events(exit_code).join("\n"));
        }
        Response::json(
            404,
            json!({ "message": format!("mock: no route for {} {}", r.method, r.target) }),
        )
    });
    base.set(mock.base.clone()).unwrap();
    mock
}

#[test]
fn creates_polls_and_downloads_with_the_expected_requests() {
    let mock = happy_mock(7, 0, |_| None);
    let o = run(&mock, &[&script("print_warn.luau"), "--format", "jsonl"]);
    assert_eq!(o.code, 0, "{}", o.stderr);

    // The task request.
    let creates = mock.matching("POST", "/luau-execution-session-tasks");
    assert_eq!(creates.len(), 1);
    let create = &creates[0];
    assert_eq!(create.path(), CREATE_LATEST);
    assert_eq!(
        create.headers.get("x-api-key").map(String::as_str),
        Some(KEY)
    );
    assert!(
        create.headers["user-agent"].starts_with("studio-run/"),
        "{:?}",
        create.headers
    );
    let body = create.json();
    let script = body["script"].as_str().unwrap();
    assert!(script.starts_with("--[[studio-run cloud wrapper]]"));
    assert!(script.contains(r#"local CHUNK_NAME = "print_warn.luau""#));
    assert!(script.contains("local CAPTURE_PRINT = true"));
    assert!(script.contains("local TRACE_MODULES = false"));
    assert!(
        script.contains(r#"careful"#),
        "the user's source is embedded"
    );
    assert_eq!(body["timeout"], "300s");
    assert_eq!(body["enableBinaryOutput"], true);
    let mut fields: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
    fields.sort();
    assert_eq!(fields, ["enableBinaryOutput", "script", "timeout"]);

    // Polling sends the key; the presigned download must not.
    for poll in mock.matching("GET", "/tasks/t1") {
        assert_eq!(poll.headers.get("x-api-key").map(String::as_str), Some(KEY));
    }
    let downloads = mock.matching("GET", "/download/t1");
    assert_eq!(downloads.len(), 1);
    assert!(!downloads[0].headers.contains_key("x-api-key"));

    // The events, with the task's details on `start`.
    let ev = jsonl(&o);
    assert_eq!(
        ev[0],
        json!({ "seq": 1, "kind": "start", "chunk": "print_warn.luau", "host": "cloud",
                "universeId": 1, "placeId": 2, "placeVersion": 7, "taskId": "t1" })
    );
    assert_eq!(ev[1]["message"], "hello");
    assert_eq!(ev.last().unwrap()["kind"], "result");
}

#[test]
fn text_output_and_exit_code_come_from_the_events() {
    let mock = happy_mock(7, 7, |_| None);
    let o = run(&mock, &[&script("print_warn.luau")]);
    assert_eq!(o.code, 7);
    assert_eq!(o.stdout.trim(), "hello");
}

#[test]
fn options_reach_the_request() {
    let mock = happy_mock(7, 0, |_| None);
    let o = run(
        &mock,
        &[
            &script("print_warn.luau"),
            "--timeout",
            "30",
            "--no-capture",
            "--trace-modules",
        ],
    );
    assert_eq!(o.code, 0);
    let body = mock.matching("POST", "/luau-execution-session-tasks")[0].json();
    assert_eq!(body["timeout"], "30s");
    let script = body["script"].as_str().unwrap();
    assert!(script.contains("local CAPTURE_PRINT = false"));
    assert!(script.contains("local TRACE_MODULES = true"));
}

#[test]
fn args_and_place_reach_the_script_context() {
    let mock = happy_mock(7, 0, |_| None);
    let o = run(
        &mock,
        &[&script("print_warn.luau"), "--arg", "k=v\"w", "--arg", "a="],
    );
    assert_eq!(o.code, 0);
    let body = mock.matching("POST", "/luau-execution-session-tasks")[0].json();
    let script = body["script"].as_str().unwrap();
    assert!(
        script.contains(r#"args = { ["a"] = "", ["k"] = "v\"w" }"#),
        "{script}"
    );
    assert!(script.contains(r#"host = "cloud""#));
    assert!(script.contains("place = { universeId = 1, placeId = 2, version = nil }"));
}

#[test]
fn bad_args_are_usage_errors() {
    let mock = MockApi::start(|_| Response::json(500, json!({})));
    let s = script("print_warn.luau");
    for args in [
        vec![s.as_str(), "--arg", "novalue"],
        vec![s.as_str(), "--arg", "=v"],
        vec![s.as_str(), "--arg", "k=1", "--arg", "k=2"],
    ] {
        let o = run(&mock, &args);
        assert_eq!(o.code, 2, "{args:?}: {}", o.stderr);
    }
    assert!(mock.requests().is_empty(), "{:?}", mock.requests());
}

#[test]
fn place_version_runs_on_that_version() {
    let mock = happy_mock(5, 0, |_| None);
    let o = run(&mock, &[&script("print_warn.luau"), "--place-version", "5"]);
    assert_eq!(o.code, 0);
    assert_eq!(
        mock.matching("POST", "/luau-execution-session-tasks")[0].path(),
        "/cloud/v2/universes/1/places/2/versions/5/luau-execution-session-tasks"
    );
}

#[test]
fn rate_limits_are_retried_after_retry_after() {
    let mock = happy_mock(7, 0, |n| {
        (n == 0).then(|| {
            Response::json(429, json!({ "message": "Too many requests" }))
                .header("Retry-After", "1")
        })
    });
    let o = run(&mock, &[&script("print_warn.luau")]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(
        mock.matching("POST", "/luau-execution-session-tasks").len(),
        2
    );
}

#[test]
fn a_rejected_key_is_an_auth_error_and_not_retried() {
    let mock = MockApi::start(|_| {
        Response::json(
            401,
            json!({ "errors": [{ "code": 0, "message": "Invalid API Key" }] }),
        )
    });
    let o = run(&mock, &[&script("print_warn.luau"), "--format", "jsonl"]);
    assert_eq!(o.code, 125);
    let ev = jsonl(&o);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(ev[0]["kind"], "error");
    assert_eq!(ev[0]["code"], "auth");
    assert!(
        ev[0]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid API Key")
    );
    assert_eq!(mock.requests().len(), 1);
}

/// A task that fails: its logs are read instead of the binary output.
fn failing_mock(code: &'static str, message: &'static str, log_lines: Vec<String>) -> MockApi {
    let path = format!("/cloud/v2/{}", task_path(7));
    let logs = format!("{path}/logs");
    MockApi::start(move |r: &Request| {
        if r.method == "POST" {
            created(7)
        } else if r.path() == path {
            Response::json(
                200,
                json!({ "path": task_path(7), "state": "FAILED", "error": { "code": code, "message": message } }),
            )
        } else if r.path() == logs {
            Response::json(
                200,
                json!({ "luauExecutionSessionTaskLogs": [{ "path": "p", "messages": log_lines }] }),
            )
        } else {
            Response::json(404, json!({}))
        }
    })
}

#[test]
fn a_timed_out_task_exits_124_with_the_output_from_its_logs() {
    let mut lines = events(0);
    lines.truncate(2); // no result: the script never finished
    lines.push("an engine message, which the wrapper also reported as an event".into());
    let mock = failing_mock("DEADLINE_EXCEEDED", "Script execution timed out.", lines);
    let o = run(
        &mock,
        &[
            &script("forever.luau"),
            "--timeout",
            "5",
            "--format",
            "jsonl",
        ],
    );
    assert_eq!(o.code, 124);
    assert!(o.stderr.contains("timed out"), "{}", o.stderr);
    let ev = jsonl(&o);
    assert_eq!(ev.len(), 2, "unwrapped log lines are ignored: {ev:?}");
    assert_eq!(ev[1]["message"], "hello");
    let log_reads = mock.matching("GET", "/logs");
    assert_eq!(log_reads.len(), 1);
    assert!(
        log_reads[0].target.contains("view=FLAT"),
        "{}",
        log_reads[0].target
    );
}

#[test]
fn a_task_that_fails_without_a_result_is_a_tool_error() {
    let mock = failing_mock("SCRIPT_ERROR", "TaskScript:5: wrapper bug", vec![]);
    let o = run(&mock, &[&script("print_warn.luau"), "--format", "jsonl"]);
    assert_eq!(o.code, 125);
    let ev = jsonl(&o);
    assert_eq!(ev.last().unwrap()["code"], "task_failed");
    assert!(
        ev.last().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("TaskScript:5")
    );
}

#[test]
fn usage_errors_make_no_requests() {
    let mock = MockApi::start(|_| Response::json(500, json!({})));
    let s = script("print_warn.luau");
    let place = fixture("marker.rbxlx");
    for args in [
        vec![s.as_str(), "--visible"],
        vec![s.as_str(), "--timeout", "301"],
        vec![s.as_str(), "--place", place.to_str().unwrap()],
        vec![s.as_str(), "--allow-upload"],
    ] {
        let o = run(&mock, &args);
        assert_eq!(o.code, 2, "{args:?}: {}", o.stderr);
    }
    // Uploading a place file isn't supported, and says what to do instead.
    let o = run(&mock, &[s.as_str(), "--place", place.to_str().unwrap()]);
    assert!(
        o.stderr.contains("--place isn't supported with --cloud"),
        "{}",
        o.stderr
    );
    // No key: a usage error too.
    let o = run_with_key(&mock, &[s.as_str()], None);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("ROBLOX_API_KEY"), "{}", o.stderr);

    assert!(mock.requests().is_empty(), "{:?}", mock.requests());
}

/// What the cloud wrapper returns: a header, the events, the binary output.
fn task_output(events: &[String], data: &[u8]) -> Vec<u8> {
    let events = events.join("\n");
    let mut out = b"SRB1".to_vec();
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(events.as_bytes());
    out.extend_from_slice(data);
    out
}

#[test]
fn inputs_upload_as_one_binary_input_and_binary_output_is_written() {
    let base = Arc::new(OnceLock::<String>::new());
    let b = base.clone();
    let mock = MockApi::start(move |r: &Request| {
        let base = b.get().unwrap();
        match (r.method.as_str(), r.path()) {
            ("POST", "/cloud/v2/universes/1/luau-execution-session-task-binary-inputs") => {
                Response::json(
                    200,
                    json!({
                        "path": "universes/1/luau-execution-session-task-binary-inputs/i1",
                        "size": r.json()["size"],
                        "uploadUri": format!("{base}/upload/i1"),
                    }),
                )
            }
            ("PUT", "/upload/i1") => Response::bytes(Vec::new()),
            ("POST", CREATE_LATEST) => created(7),
            ("GET", p) if p == format!("/cloud/v2/{}", task_path(7)) => complete(base, 7, 0),
            ("GET", "/download/t1") => Response::bytes(task_output(&events(0), b"\x00out\xff")),
            _ => Response::json(404, json!({ "message": "mock: no route" })),
        }
    });
    base.set(mock.base.clone()).unwrap();

    let (inputs, out, _) = common::binary_io_case("mock-io", 10);
    let o = run(
        &mock,
        &[
            &script("print_warn.luau"),
            "--input",
            &inputs[0],
            "--input",
            &inputs[1],
            "--binary-output",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout.trim(), "hello");
    assert_eq!(std::fs::read(&out).unwrap(), b"\x00out\xff");

    // One input, sized to and holding both files, packed with an index.
    let a = std::fs::read(inputs[0].split_once('=').unwrap().1).unwrap();
    let b = std::fs::read(inputs[1].split_once('=').unwrap().1).unwrap();
    let mut packed = b"SRI1\x02\0\0\0".to_vec();
    for (name, data) in [("a", &a), ("b", &b)] {
        packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
        packed.extend_from_slice(name.as_bytes());
        packed.extend_from_slice(&(data.len() as u32).to_le_bytes());
        packed.extend_from_slice(data);
    }
    let creates = mock.matching("POST", "/luau-execution-session-task-binary-inputs");
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].json(), json!({ "size": packed.len() }));
    assert_eq!(
        creates[0].headers.get("x-api-key").map(String::as_str),
        Some(KEY)
    );
    // The presigned upload gets the bytes with PUT, and never the key.
    let uploads = mock.matching("PUT", "/upload/i1");
    assert_eq!(uploads.len(), 1);
    assert!(!uploads[0].headers.contains_key("x-api-key"));
    assert!(uploads[0].body == packed);

    let task = mock.matching("POST", "/luau-execution-session-tasks")[0].json();
    assert_eq!(
        task["binaryInput"],
        "universes/1/luau-execution-session-task-binary-inputs/i1"
    );
    assert!(
        task["script"]
            .as_str()
            .unwrap()
            .contains("local SEND_BINARY_OUTPUT = true")
    );
}

#[test]
fn without_inputs_no_binary_input_is_created() {
    let mock = happy_mock(7, 0, |_| None);
    let o = run(&mock, &[&script("print_warn.luau")]);
    assert_eq!(o.code, 0);
    assert!(
        mock.matching("POST", "/luau-execution-session-task-binary-inputs")
            .is_empty()
    );
}

#[test]
fn binary_output_from_logs_only_is_a_tool_error() {
    // The download fails, so the events come from the logs, without the binary output.
    let base = Arc::new(OnceLock::<String>::new());
    let b = base.clone();
    let mock = MockApi::start(move |r: &Request| {
        let base = b.get().unwrap();
        match (r.method.as_str(), r.path()) {
            ("POST", CREATE_LATEST) => created(7),
            ("GET", p) if p == format!("/cloud/v2/{}", task_path(7)) => complete(base, 7, 0),
            ("GET", "/download/t1") => Response::json(404, json!({ "message": "gone" })),
            ("GET", p) if p.ends_with("/logs") => Response::json(
                200,
                json!({ "luauExecutionSessionTaskLogs": [{ "messages": events(0) }] }),
            ),
            _ => Response::json(404, json!({ "message": "mock: no route" })),
        }
    });
    base.set(mock.base.clone()).unwrap();
    let (_, out, _) = common::binary_io_case("mock-lost", 0);
    let o = run(
        &mock,
        &[
            &script("print_warn.luau"),
            "--binary-output",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(o.code, 125, "{}", o.stderr);
    assert!(o.stderr.contains("binary output"), "{}", o.stderr);
    assert!(!out.exists());
}

#[test]
fn bad_inputs_are_usage_errors() {
    let mock = happy_mock(7, 0, |_| None);
    let missing = std::env::temp_dir().join("studio-run-test-no-such-input");
    for args in [
        vec!["--input", "noequals"],
        vec!["--input", "=x"],
        vec!["--input", &format!("a={}", missing.display())],
    ] {
        let mut all = vec![script("print_warn.luau")];
        all.extend(args.iter().map(|s| s.to_string()));
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        let o = run(&mock, &all);
        assert_eq!(o.code, 2, "{args:?}: {}", o.stderr);
    }
    assert!(mock.requests().is_empty());
}
