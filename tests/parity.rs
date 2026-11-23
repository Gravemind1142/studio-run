//! Parity: the same scripts run locally in Studio and as Open Cloud tasks
//! must produce the same events, apart from the differences listed in
//! `ALLOWED` below.
//!
//! It needs both a logged-in Studio and the test experience and key from
//! docs/cloud-testing.md, so it's ignored by default and can't run in CI:
//!
//!     cargo test --test parity -- --ignored --nocapture
//!
//! The local side runs `studio-run run --format jsonl` as usual.
//!
//! Task creation is rate limited (5 per minute per key owner), so the cloud
//! side batches several scripts per task while still exercising studio-run's
//! own cloud wrapper and event handling:
//!
//! 1. Capture: `studio-run run --cloud` runs against a local mock
//!    (tests/common/mock.rs), which records the task script studio-run
//!    generates for the script and refuses to create the task.
//! 2. Batch: the captured task scripts run one after another in a single real
//!    task, each inside its own function. Their binary outputs (the wrapper's
//!    event lines) come back together.
//! 3. Replay: `studio-run run --cloud` runs again against a mock that answers
//!    with a completed task whose binary output is that script's lines, so
//!    studio-run parses and prints them exactly as after a task of its own.
//!
//! A script that can't share a task (one that needs its own timeout, binary
//! input or place version, or that leaves threads running after it returns)
//! must set `own_task`.

mod common;

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::thread;

use common::cloud::{TaskOptions, binary_output, run_task};
use common::mock::{MockApi, Request, Response};
use common::{Output, exe, fixture, studio_run};
use serde_json::{Value, json};

/// One script to run on both hosts.
struct Case {
    /// A file in tests/fixtures.
    file: &'static str,
    /// Extra `run` arguments, for both hosts.
    args: &'static [&'static str],
    /// Run in a task of its own instead of a batch.
    own_task: bool,
}

const fn case(file: &'static str) -> Case {
    Case {
        file,
        args: &[],
        own_task: false,
    }
}

const CASES: &[Case] = &[
    // Output
    case("print_warn.luau"),
    Case {
        args: &["--no-capture"],
        ..case("print_warn.luau")
    },
    case("output_kinds.luau"),
    case("unicode.luau"),
    case("bom.luau"),
    case("long_output.luau"),
    // Errors
    case("error.luau"),
    case("nested_error.luau"),
    case("compile_error.luau"),
    case("async_error.luau"),
    // Yielding
    case("yield.luau"),
    // Return values and exit codes
    case("exit_code.luau"),
    case("return_false.luau"),
    case("return_table.luau"),
    case("return_multiple.luau"),
    // Inputs
    Case {
        args: &[
            "--arg",
            "name=a=b",
            "--arg",
            "empty=",
            "--arg",
            "uni=héllo ✓",
        ],
        ..case("args.luau")
    },
    // Types
    case("export_type.luau"),
];

/// A known difference between the hosts, with the finding that explains it.
/// `check` asserts the difference is exactly the one described. If the hosts
/// start agreeing on the script, the test fails so the entry gets removed.
struct Difference {
    file: &'static str,
    finding: &'static str,
    what: &'static str,
    check: fn(local: &Run, cloud: &Run),
}

const ALLOWED: &[Difference] = &[Difference {
    file: "compile_error.luau",
    finding: "docs/cloud-behavior.md#compiling-the-users-source-as-a-modulescript",
    what: "the cloud closes the source with `\\nend`, so a compile error at the \
           end of the file is reported a line later, at `end` instead of <eof>",
    check: |local, cloud| {
        // Everything but the message is the same.
        let without_errors = |r: &Run| {
            r.events
                .iter()
                .filter(|e| e["level"] != "error")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(local.code, cloud.code);
        assert_eq!(without_errors(local), without_errors(cloud));
        assert_eq!(
            local.script_output("error"),
            ["compile_error.luau:2: Expected identifier when parsing expression, got <eof>"]
        );
        assert_eq!(
            cloud.script_output("error"),
            ["compile_error.luau:3: Expected identifier when parsing expression, got 'end'"]
        );
    },
}];

/// Scripts per batch task. Small enough that one misbehaving script can't
/// take many others down with it.
const BATCH: usize = 6;

/// One host's run of a case, normalized.
#[derive(Debug, PartialEq)]
struct Run {
    code: i32,
    events: Vec<Value>,
}

impl Run {
    fn from_output(o: &Output) -> Run {
        let events = o
            .stdout
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("jsonl line {l:?}: {e}")))
            .filter_map(normalize)
            .collect();
        Run {
            code: o.code,
            events,
        }
    }

    fn script_output(&self, level: &str) -> Vec<&str> {
        self.events
            .iter()
            .filter(|e| e["kind"] == "output" && e["source"] == "script" && e["level"] == level)
            .map(|e| e["message"].as_str().unwrap_or(""))
            .collect()
    }
}

/// Drops what legitimately differs between hosts: `seq` (the hosts number
/// events differently once `other` output is dropped), `other` output (the
/// engine's own messages differ between Studio and a task), and where the
/// script ran on `start`. Events carry no timing.
fn normalize(mut ev: Value) -> Option<Value> {
    if ev["kind"] == "output" && ev["source"] == "other" {
        return None;
    }
    let obj = ev.as_object_mut()?;
    obj.remove("seq");
    if obj.get("kind").is_some_and(|k| k == "start") {
        for key in ["host", "universeId", "placeId", "placeVersion", "taskId"] {
            obj.remove(key);
        }
    }
    Some(ev)
}

fn label(c: &Case) -> String {
    if c.args.is_empty() {
        c.file.to_string()
    } else {
        format!("{} {}", c.file, c.args.join(" "))
    }
}

fn jsonl_args(c: &Case) -> Vec<String> {
    let mut args = vec![
        fixture(c.file).to_str().unwrap().to_string(),
        "--format".into(),
        "jsonl".into(),
    ];
    args.extend(c.args.iter().map(|a| a.to_string()));
    args
}

// ---------------------------------------------------------------------------
// Local
// ---------------------------------------------------------------------------

fn run_local(c: &Case) -> Output {
    let mut args = vec!["run".to_string()];
    args.extend(jsonl_args(c));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    studio_run(&args)
}

// ---------------------------------------------------------------------------
// Cloud: capture, batch, replay
// ---------------------------------------------------------------------------

const MOCK_KEY: &str = "parity-mock-key";

/// `studio-run run --cloud <case>` against `mock`.
fn run_against_mock(mock: &MockApi, c: &Case) -> Output {
    let out = Command::new(exe())
        .args([
            "run",
            "--cloud",
            "--poll-interval",
            "1",
            "--universe-id",
            "1",
            "--place-id",
            "2",
        ])
        .args(jsonl_args(c))
        .env("STUDIO_RUN_TEST_API_BASE", &mock.base)
        .env("ROBLOX_API_KEY", MOCK_KEY)
        .env_remove("STUDIO_RUN_HOST")
        // Outside the checkout, so a studio-run.toml there can't change the run.
        .current_dir(std::env::temp_dir())
        .output()
        .expect("spawn studio-run");
    Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn is_create(r: &Request) -> bool {
    r.method == "POST" && r.path().ends_with("/luau-execution-session-tasks")
}

/// The task script studio-run would send for `c`.
fn capture(c: &Case) -> String {
    let mock = MockApi::start(|r: &Request| {
        if is_create(r) {
            Response::json(
                400,
                json!({ "code": "INVALID_ARGUMENT", "message": "parity: capture only" }),
            )
        } else {
            Response::json(
                404,
                json!({ "code": "NOT_FOUND", "message": "parity mock" }),
            )
        }
    });
    let o = run_against_mock(&mock, c);
    let creates: Vec<_> = mock.requests().into_iter().filter(is_create).collect();
    assert_eq!(
        creates.len(),
        1,
        "{}: expected one create request\n{}",
        label(c),
        o.stderr
    );
    creates[0].json()["script"]
        .as_str()
        .expect("create request has a script")
        .to_string()
}

/// One task script that runs `scripts` in order and returns each one's
/// binary output, as a JSON array of `{ ok, output }` in the binary output.
fn batch_script(scripts: &[String]) -> String {
    let mut s =
        String::from("local HttpService = game:GetService(\"HttpService\")\nlocal outputs = {}\n");
    for (i, script) in scripts.iter().enumerate() {
        s.push_str(&format!(
            "do\n\
             local ok, out = pcall(function()\n{script}\nend)\n\
             outputs[{n}] = if ok then {{ ok = true, output = buffer.tostring(out.BinaryOutput) }} \
             else {{ ok = false, output = tostring(out) }}\n\
             end\n",
            n = i + 1
        ));
    }
    s.push_str(
        "return { BinaryOutput = buffer.fromstring(HttpService:JSONEncode(outputs)), \
         ReturnValues = { #outputs } }\n",
    );
    s
}

/// Runs the scripts in one real task and returns each one's wrapper output.
fn run_batch(scripts: &[String]) -> Vec<String> {
    let task = run_task(
        &batch_script(scripts),
        &TaskOptions {
            timeout: Some(300),
            enable_binary_output: true,
            ..Default::default()
        },
    );
    assert_eq!(
        task["state"], "COMPLETE",
        "batch task didn't complete: {}",
        task["error"]
    );
    let outputs: Vec<Value> =
        serde_json::from_slice(&binary_output(&task)).expect("batch output is JSON");
    assert_eq!(outputs.len(), scripts.len());
    outputs
        .into_iter()
        .map(|o| {
            let text = o["output"].as_str().unwrap_or("").to_string();
            assert!(o["ok"] == true, "a wrapper failed in the batch: {text}");
            text
        })
        .collect()
}

/// studio-run's output for `c`, given what its task wrote to the binary output.
fn replay(c: &Case, binary: String) -> Output {
    let base = Arc::new(OnceLock::<String>::new());
    let b = base.clone();
    let path = "universes/1/places/2/versions/1/luau-execution-sessions/s1/tasks/t1";
    let mock = MockApi::start(move |r: &Request| {
        if is_create(r) {
            return Response::json(
                200,
                json!({
                    "path": path,
                    "state": "COMPLETE",
                    "binaryOutputUri": format!("{}/download/t1", b.get().unwrap()),
                }),
            );
        }
        if r.method == "GET" && r.path() == "/download/t1" {
            return Response::bytes(binary.clone());
        }
        Response::json(
            404,
            json!({ "code": "NOT_FOUND", "message": "parity mock" }),
        )
    });
    base.set(mock.base.clone()).unwrap();
    run_against_mock(&mock, c)
}

/// Runs every case in the cloud, `BATCH` to a task.
fn run_cloud(cases: &[&Case]) -> Vec<Output> {
    let scripts: Vec<String> = cases.iter().map(|c| capture(c)).collect();
    let mut binaries = vec![String::new(); cases.len()];
    let (alone, batched): (Vec<usize>, Vec<usize>) =
        (0..cases.len()).partition(|&i| cases[i].own_task);
    let mut groups: Vec<Vec<usize>> = batched.chunks(BATCH).map(<[usize]>::to_vec).collect();
    groups.extend(alone.into_iter().map(|i| vec![i]));
    for group in groups {
        let names: Vec<String> = group.iter().map(|&i| label(cases[i])).collect();
        eprintln!("cloud: one task for {names:?}");
        let batch: Vec<String> = group.iter().map(|&i| scripts[i].clone()).collect();
        for (i, out) in group.iter().zip(run_batch(&batch)) {
            binaries[*i] = out;
        }
    }
    cases
        .iter()
        .zip(binaries)
        .map(|(c, binary)| replay(c, binary))
        .collect()
}

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a logged-in Studio and an Open Cloud test experience; see docs/cloud-testing.md"]
fn local_and_cloud_events_match() {
    let cases: Vec<&Case> = CASES.iter().collect();
    for d in ALLOWED {
        assert!(
            cases.iter().any(|c| c.file == d.file),
            "allowed difference for {}, which isn't a case",
            d.file
        );
    }

    // Both hosts at once: the cloud side is mostly waiting.
    let (local, cloud) = thread::scope(|s| {
        let cloud = s.spawn(|| run_cloud(&cases));
        let local: Vec<_> = cases
            .iter()
            .map(|c| s.spawn(move || run_local(c)))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>();
        (local, cloud.join().unwrap())
    });

    let mut failures = Vec::new();
    let allowed: HashMap<&str, &Difference> = ALLOWED.iter().map(|d| (d.file, d)).collect();
    for ((c, l), cl) in cases.iter().zip(&local).zip(&cloud) {
        let name = label(c);
        let (l, cl) = (Run::from_output(l), Run::from_output(cl));
        eprintln!("--- {name}\nlocal: {l:?}\ncloud: {cl:?}");
        match allowed.get(c.file) {
            Some(d) if l == cl => failures.push(format!(
                "{name}: the hosts now agree; remove the allowed difference ({})",
                d.what
            )),
            Some(d) => {
                let checked = std::panic::catch_unwind(|| (d.check)(&l, &cl));
                if checked.is_err() {
                    failures.push(format!(
                        "{name}: differs, but not as allowed ({}; see {})",
                        d.what, d.finding
                    ));
                }
            }
            None if l != cl => failures.push(format!(
                "{name}: events differ\n  local: {l:?}\n  cloud: {cl:?}"
            )),
            None => {}
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
