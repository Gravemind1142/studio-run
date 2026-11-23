//! Settings for the tests that run Open Cloud Luau Execution tasks.
//!
//! Read from environment variables, or from a `.env` file at the repo root
//! (gitignored; copy `.env.example`). A variable that's set in the environment
//! wins over the file. See docs/cloud-testing.md.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::root;

pub const UNIVERSE_ID: &str = "STUDIO_RUN_TEST_UNIVERSE_ID";
pub const PLACE_ID: &str = "STUDIO_RUN_TEST_PLACE_ID";
pub const API_KEY: &str = "STUDIO_RUN_TEST_API_KEY";

pub struct CloudEnv {
    pub universe_id: u64,
    pub place_id: u64,
    pub api_key: String,
}

// Never print the key, even in a failed assertion.
impl fmt::Debug for CloudEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudEnv")
            .field("universe_id", &self.universe_id)
            .field("place_id", &self.place_id)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

/// The test experience and key. Panics with setup instructions if any is missing.
pub fn cloud_env() -> &'static CloudEnv {
    static ENV: OnceLock<CloudEnv> = OnceLock::new();
    ENV.get_or_init(|| match load(&dotenv_file()) {
        Ok(env) => env,
        Err(problems) => panic!(
            "cloud tests need a test experience and API key:\n  {}\n\
             Set them in the environment or in .env at the repo root \
             (copy .env.example). See docs/cloud-testing.md.",
            problems.join("\n  ")
        ),
    })
}

/// Values from `.env`, without touching the process environment (tests run
/// on several threads, and setting variables there isn't thread-safe).
fn dotenv_file() -> HashMap<String, String> {
    let path = root().join(".env");
    let Ok(iter) = dotenvy::from_path_iter(&path) else {
        return HashMap::new(); // no file: environment only
    };
    iter.map(|item| item.unwrap_or_else(|e| panic!("couldn't read {}: {e}", path.display())))
        .collect()
}

fn load(file: &HashMap<String, String>) -> Result<CloudEnv, Vec<String>> {
    let get = |name: &str| {
        std::env::var(name)
            .ok()
            .or_else(|| file.get(name).cloned())
            .filter(|v| !v.trim().is_empty())
    };
    let mut problems = Vec::new();
    let mut id = |name: &str| match get(name) {
        None => {
            problems.push(format!("{name} is not set"));
            0
        }
        Some(v) => v.trim().parse().unwrap_or_else(|_| {
            problems.push(format!("{name} is not a number: {v:?}"));
            0
        }),
    };
    let universe_id = id(UNIVERSE_ID);
    let place_id = id(PLACE_ID);
    let api_key = get(API_KEY).unwrap_or_else(|| {
        problems.push(format!("{API_KEY} is not set"));
        String::new()
    });
    if problems.is_empty() {
        Ok(CloudEnv {
            universe_id,
            place_id,
            api_key: api_key.trim().to_string(),
        })
    } else {
        Err(problems)
    }
}

// ---------------------------------------------------------------------------
// A minimal Open Cloud client, just enough to probe Luau Execution.
// ---------------------------------------------------------------------------

const BASE: &str = "https://apis.roblox.com";
/// Roblox allows 5 task creations per minute per key owner.
const CREATE_SPACING: Duration = Duration::from_secs(13);
static LAST_CREATE: Mutex<Option<Instant>> = Mutex::new(None);

/// An HTTP response: status and parsed JSON body (or the text, as a JSON string).
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub retry_after: Option<u64>,
}

impl Reply {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body, or a panic naming the request that failed.
    pub fn expect(self, what: &str) -> Value {
        assert!(self.ok(), "{what}: HTTP {} {}", self.status, self.body);
        self.body
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(120))
            .build()
    })
}

fn reply(result: Result<ureq::Response, ureq::Error>) -> Reply {
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => panic!("request failed: {e}"),
    };
    let status = resp.status();
    let retry_after = resp
        .header("retry-after")
        .and_then(|v| v.trim().parse().ok());
    let text = resp.into_string().unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
    Reply {
        status,
        body,
        retry_after,
    }
}

/// Sends a request with the test key. `url` may be a full URL or a path under BASE.
/// Only Roblox's own hosts get the key.
pub fn request(method: &str, url: &str, body: Option<&Value>) -> Reply {
    let url = if url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{BASE}{url}")
    };
    assert!(url.starts_with(BASE), "refusing to send the key to {url}");
    let req = agent()
        .request(method, &url)
        .set("x-api-key", &cloud_env().api_key);
    reply(match body {
        Some(b) => req.send_json(b),
        None => req.call(),
    })
}

/// Like `request`, retrying 429s after `retry-after` (or 15 s) up to `tries` times.
fn request_retrying(method: &str, url: &str, body: Option<&Value>, tries: u32) -> Reply {
    let mut r = request(method, url, body);
    for _ in 1..tries {
        if r.status != 429 {
            break;
        }
        let wait = r.retry_after.unwrap_or(15).clamp(1, 120);
        eprintln!("cloud: 429 on {method} {url}; retrying in {wait}s");
        std::thread::sleep(Duration::from_secs(wait));
        r = request(method, url, body);
    }
    r
}

/// Waits so task creations stay under the rate limit, across all tests in this binary.
pub fn pace_create() {
    let mut last = LAST_CREATE.lock().unwrap();
    if let Some(t) = *last {
        let since = t.elapsed();
        if since < CREATE_SPACING {
            std::thread::sleep(CREATE_SPACING - since);
        }
    }
    *last = Some(Instant::now());
}

#[derive(Default)]
pub struct TaskOptions {
    /// Run on this place version instead of the latest.
    pub version: Option<u64>,
    /// Task timeout in seconds (API default: 300).
    pub timeout: Option<u64>,
    /// A binary input's `path`, from `create_binary_input`.
    pub binary_input: Option<String>,
    pub enable_binary_output: bool,
}

/// Creates a task and returns the created task resource.
pub fn create_task(script: &str, opts: &TaskOptions) -> Value {
    let env = cloud_env();
    let url = match opts.version {
        Some(v) => format!(
            "/cloud/v2/universes/{}/places/{}/versions/{v}/luau-execution-session-tasks",
            env.universe_id, env.place_id
        ),
        None => format!(
            "/cloud/v2/universes/{}/places/{}/luau-execution-session-tasks",
            env.universe_id, env.place_id
        ),
    };
    let mut body = json!({ "script": script });
    if let Some(t) = opts.timeout {
        body["timeout"] = json!(format!("{t}s"));
    }
    if let Some(p) = &opts.binary_input {
        body["binaryInput"] = json!(p);
    }
    if opts.enable_binary_output {
        body["enableBinaryOutput"] = json!(true);
    }
    pace_create();
    request_retrying("POST", &url, Some(&body), 6).expect("create task")
}

pub fn task_url(task: &Value) -> String {
    format!(
        "/cloud/v2/{}",
        task["path"].as_str().expect("task has a path")
    )
}

pub fn get_task(task: &Value) -> Value {
    request_retrying("GET", &task_url(task), None, 6).expect("get task")
}

pub fn is_done(task: &Value) -> bool {
    !matches!(task["state"].as_str(), Some("QUEUED" | "PROCESSING"))
}

/// Polls until the task leaves QUEUED/PROCESSING, returning the final resource.
pub fn wait_task(task: &Value, limit: Duration) -> Value {
    let start = Instant::now();
    let mut t = task.clone();
    while !is_done(&t) {
        assert!(
            start.elapsed() < limit,
            "task still {} after {limit:?}",
            t["state"]
        );
        std::thread::sleep(Duration::from_secs(2));
        t = get_task(task);
    }
    t
}

/// Creates a task, waits for it, and returns the final resource.
pub fn run_task(script: &str, opts: &TaskOptions) -> Value {
    let task = create_task(script, opts);
    wait_task(
        &task,
        Duration::from_secs(opts.timeout.unwrap_or(300) + 120),
    )
}

/// All log entries of a task, in the given view ("FLAT" or "STRUCTURED"), as returned.
pub fn task_logs(task: &Value, view: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut token = String::new();
    loop {
        let mut url = format!("{}/logs?view={view}&maxPageSize=10000", task_url(task));
        if !token.is_empty() {
            url.push_str(&format!("&pageToken={token}"));
        }
        let page = request_retrying("GET", &url, None, 6).expect("list logs");
        if let Some(items) = page["luauExecutionSessionTaskLogs"].as_array() {
            out.extend(items.iter().cloned());
        }
        match page["nextPageToken"].as_str() {
            Some(t) if !t.is_empty() => token = t.to_string(),
            _ => return out,
        }
    }
}

/// Creates a binary input of `data` and uploads it. Returns the input's `path`
/// and the HTTP method that the upload URL accepted.
pub fn create_binary_input(data: &[u8]) -> (String, &'static str) {
    let env = cloud_env();
    let url = format!(
        "/cloud/v2/universes/{}/luau-execution-session-task-binary-inputs",
        env.universe_id
    );
    let input = request_retrying("POST", &url, Some(&json!({ "size": data.len() })), 6)
        .expect("create binary input");
    let path = input["path"]
        .as_str()
        .expect("binary input path")
        .to_string();
    let upload = input["uploadUri"].as_str().expect("binary input uploadUri");
    // The upload URL is presigned storage, not apis.roblox.com: never send the key there.
    for method in ["PUT", "POST"] {
        let r = reply(
            agent()
                .request(method, upload)
                .set("Content-Type", "application/octet-stream")
                .send_bytes(data),
        );
        if r.ok() {
            return (path, method);
        }
        eprintln!(
            "cloud: binary input upload with {method}: HTTP {} {}",
            r.status, r.body
        );
    }
    panic!("couldn't upload the binary input");
}

/// Downloads a finished task's binary output.
pub fn binary_output(task: &Value) -> Vec<u8> {
    let uri = task["binaryOutputUri"]
        .as_str()
        .expect("task has binaryOutputUri");
    // Presigned URL: no key.
    let resp = agent().get(uri).call().expect("download binary output");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).unwrap();
    bytes
}
