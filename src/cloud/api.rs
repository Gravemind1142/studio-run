//! A minimal Open Cloud client: upload a task's binary input, create and poll
//! a Luau Execution task, read its logs and download its binary output.
//! Behaviour it relies on is in docs/internals/cloud-behavior.md.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BASE: &str = "https://apis.roblox.com";

/// Test-only: point the client at a local mock server (tests/cloud_mock.rs)
/// instead of Open Cloud. Only loopback addresses are accepted, so this can't
/// be used to send the key anywhere else.
pub const BASE_OVERRIDE_ENV: &str = "STUDIO_RUN_TEST_API_BASE";

fn base_url() -> String {
    match std::env::var(BASE_OVERRIDE_ENV) {
        Ok(url) if is_loopback(&url) => url.trim_end_matches('/').to_string(),
        _ => BASE.to_string(),
    }
}

/// `http://127.0.0.1:<port>` or `http://localhost:<port>`, optionally with a
/// path. Anything else after the host (`@user-info`, more host) is rejected.
fn is_loopback(url: &str) -> bool {
    let Some(rest) = ["http://127.0.0.1:", "http://localhost:"]
        .iter()
        .find_map(|p| url.strip_prefix(p))
    else {
        return false;
    };
    let (port, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && !path.contains('@')
}

/// A failed request, with a stable `code` for the `error` event.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// `auth`, `permission`, `not_found`, `invalid_request`, `rate_limited`,
    /// `server`, `http` or `network`.
    pub code: &'static str,
    pub status: Option<u16>,
    pub message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    fn retryable(&self) -> bool {
        matches!(self.code, "rate_limited" | "server" | "network")
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(s) => write!(f, "HTTP {s}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

pub fn classify(status: u16) -> &'static str {
    match status {
        400 => "invalid_request",
        401 => "auth",
        403 => "permission",
        404 => "not_found",
        429 => "rate_limited",
        500..=599 => "server",
        _ => "http",
    }
}

/// The message in an Open Cloud error body, in either of its two shapes.
fn error_message(body: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let msg = v["message"]
        .as_str()
        .or_else(|| v["errors"][0]["message"].as_str())
        .filter(|m| !m.is_empty());
    match msg {
        Some(m) => m.to_string(),
        None if body.trim().is_empty() => "(no message)".to_string(),
        None => body.chars().take(300).collect(),
    }
}

pub struct Client {
    agent: ureq::Agent,
    key: String,
    base: String,
}

/// The IDs in a task's `path`
/// (`universes/U/places/P/versions/V/luau-execution-sessions/S/tasks/T`).
#[derive(Debug, PartialEq)]
pub struct TaskPath {
    pub version: Option<u64>,
    pub task_id: Option<String>,
}

pub fn parse_task_path(path: &str) -> TaskPath {
    let parts: Vec<&str> = path.split('/').collect();
    let after = |name: &str| {
        parts
            .iter()
            .position(|p| *p == name)
            .and_then(|i| parts.get(i + 1))
            .map(|s| s.to_string())
    };
    TaskPath {
        version: after("versions").and_then(|v| v.parse().ok()),
        task_id: after("tasks"),
    }
}

impl Client {
    pub fn new(key: String) -> Client {
        Client {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(30))
                .timeout(Duration::from_secs(300))
                .user_agent(concat!("studio-run/", env!("CARGO_PKG_VERSION")))
                .build(),
            key,
            base: base_url(),
        }
    }

    fn finish(result: Result<ureq::Response, ureq::Error>) -> Result<String, ApiError> {
        match result {
            Ok(resp) => resp.into_string().map_err(|e| ApiError {
                code: "network",
                status: None,
                message: format!("couldn't read the response: {e}"),
                retry_after: None,
            }),
            Err(ureq::Error::Status(status, resp)) => {
                let retry_after = resp
                    .header("retry-after")
                    .and_then(|v| v.trim().parse().ok());
                let body = resp.into_string().unwrap_or_default();
                Err(ApiError {
                    code: classify(status),
                    status: Some(status),
                    message: error_message(&body),
                    retry_after,
                })
            }
            Err(e) => Err(ApiError {
                code: "network",
                status: None,
                message: e.to_string(),
                retry_after: None,
            }),
        }
    }

    /// One JSON request to `apis.roblox.com` with the key.
    fn json(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        let req = self
            .agent
            .request(method, &format!("{}{path}", self.base))
            .set("x-api-key", &self.key);
        let text = Self::finish(match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        })?;
        serde_json::from_str(&text).map_err(|e| ApiError {
            code: "http",
            status: None,
            message: format!(
                "unexpected response ({e}): {}",
                text.chars().take(200).collect::<String>()
            ),
            retry_after: None,
        })
    }

    /// Runs `f`, retrying errors that are safe to retry until `deadline`, with
    /// backoff that respects `retry-after`.
    fn retrying<T>(
        deadline: Instant,
        mut f: impl FnMut() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut backoff = 2;
        loop {
            match f() {
                Err(e) if e.retryable() => {
                    let wait = Duration::from_secs(e.retry_after.unwrap_or(backoff).clamp(1, 60));
                    if Instant::now() + wait >= deadline {
                        return Err(e);
                    }
                    std::thread::sleep(wait);
                    backoff = (backoff * 2).min(30);
                }
                other => return other,
            }
        }
    }

    /// Creates a task on `version` (or the latest version) and returns it.
    #[allow(clippy::too_many_arguments)]
    pub fn create_task(
        &self,
        universe_id: u64,
        place_id: u64,
        version: Option<u64>,
        script: &str,
        binary_input: Option<&str>,
        timeout_secs: u64,
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        let path = match version {
            Some(v) => format!(
                "/cloud/v2/universes/{universe_id}/places/{place_id}/versions/{v}/luau-execution-session-tasks"
            ),
            None => format!(
                "/cloud/v2/universes/{universe_id}/places/{place_id}/luau-execution-session-tasks"
            ),
        };
        let mut body = json!({
            "script": script,
            "timeout": format!("{timeout_secs}s"),
            "enableBinaryOutput": true,
        });
        if let Some(input) = binary_input {
            body["binaryInput"] = json!(input);
        }
        // A failed create never created a task, so retrying can't run it twice.
        Self::retrying(deadline, || self.json("POST", &path, Some(&body)))
    }

    /// Creates a binary input holding `data` and uploads it. Returns its
    /// `path`, for `create_task`. It stays usable for 15 minutes.
    pub fn create_binary_input(
        &self,
        universe_id: u64,
        data: &[u8],
        deadline: Instant,
    ) -> Result<String, ApiError> {
        let path =
            format!("/cloud/v2/universes/{universe_id}/luau-execution-session-task-binary-inputs");
        let body = json!({ "size": data.len() });
        // An unused input does no harm, so creating one is safe to retry.
        let input = Self::retrying(deadline, || self.json("POST", &path, Some(&body)))?;
        let (Some(input_path), Some(upload)) =
            (input["path"].as_str(), input["uploadUri"].as_str())
        else {
            return Err(ApiError {
                code: "http",
                status: None,
                message: format!("the created binary input has no path or uploadUri: {input}"),
                retry_after: None,
            });
        };
        // The upload URL is presigned storage: no key is sent. PUT is idempotent.
        Self::retrying(deadline, || {
            Self::finish(
                self.agent
                    .put(upload)
                    .set("Content-Type", "application/octet-stream")
                    .send_bytes(data),
            )
        })?;
        Ok(input_path.to_string())
    }

    /// The task's current state.
    pub fn get_task(&self, task_path: &str, deadline: Instant) -> Result<Value, ApiError> {
        let path = format!("/cloud/v2/{task_path}");
        Self::retrying(deadline, || self.json("GET", &path, None))
    }

    /// All of a finished task's log messages, oldest first.
    pub fn logs(&self, task_path: &str, deadline: Instant) -> Result<Vec<String>, ApiError> {
        let mut out = Vec::new();
        let mut token = String::new();
        loop {
            let mut path = format!("/cloud/v2/{task_path}/logs?view=FLAT&maxPageSize=10000");
            if !token.is_empty() {
                path.push_str("&pageToken=");
                path.push_str(&token);
            }
            let page = Self::retrying(deadline, || self.json("GET", &path, None))?;
            for entry in page["luauExecutionSessionTaskLogs"]
                .as_array()
                .into_iter()
                .flatten()
            {
                for m in entry["messages"].as_array().into_iter().flatten() {
                    if let Some(s) = m.as_str() {
                        out.push(s.to_string());
                    }
                }
            }
            match page["nextPageToken"].as_str() {
                Some(t) if !t.is_empty() => token = t.to_string(),
                _ => return Ok(out),
            }
        }
    }

    /// Downloads a task's binary output. The URL is presigned: no key is sent.
    pub fn download(&self, uri: &str, deadline: Instant) -> Result<Vec<u8>, ApiError> {
        Self::retrying(deadline, || {
            let resp = match self.agent.get(uri).call() {
                Ok(r) => r,
                Err(e) => return Self::finish(Err(e)).map(|_| Vec::new()),
            };
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).map_err(|e| {
                ApiError {
                    code: "network",
                    status: None,
                    message: format!("couldn't download the binary output: {e}"),
                    retry_after: None,
                }
            })?;
            Ok(bytes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_task_paths() {
        let p = parse_task_path(
            "universes/1/places/2/versions/9/luau-execution-sessions/abc/tasks/def",
        );
        assert_eq!(
            p,
            TaskPath {
                version: Some(9),
                task_id: Some("def".into())
            }
        );
        assert_eq!(parse_task_path("nonsense").version, None);
    }

    #[test]
    fn reads_both_error_shapes() {
        assert_eq!(
            error_message(r#"{"code":"INVALID_ARGUMENT","message":"bad"}"#),
            "bad"
        );
        assert_eq!(
            error_message(r#"{"errors":[{"code":0,"message":"Scope not authorized."}]}"#),
            "Scope not authorized."
        );
        assert_eq!(
            error_message(r#"{"errors":[{"code":0,"message":""}]}"#),
            r#"{"errors":[{"code":0,"message":""}]}"#
        );
        assert_eq!(error_message(""), "(no message)");
    }

    #[test]
    fn the_test_base_override_only_accepts_loopback() {
        assert!(is_loopback("http://127.0.0.1:5000"));
        assert!(is_loopback("http://localhost:5000/"));
        assert!(!is_loopback("https://apis.roblox.com"));
        assert!(!is_loopback("http://127.0.0.1.evil.example:80"));
        assert!(!is_loopback("http://example.com/http://127.0.0.1:1"));
        assert!(!is_loopback("https://127.0.0.1:5000"));
        assert!(!is_loopback("http://localhost:5000@evil.example/"));
        assert!(!is_loopback("http://127.0.0.1:80.evil.example"));
        assert!(!is_loopback("http://127.0.0.1:/"));
    }

    #[test]
    fn classifies_statuses() {
        assert_eq!(classify(401), "auth");
        assert_eq!(classify(403), "permission");
        assert_eq!(classify(429), "rate_limited");
        assert_eq!(classify(503), "server");
    }

    #[test]
    fn retries_only_retryable_errors_until_the_deadline() {
        let err = |code| ApiError {
            code,
            status: None,
            message: String::new(),
            retry_after: Some(1),
        };
        let mut calls = 0;
        let r: Result<(), _> = Client::retrying(Instant::now() + Duration::from_secs(30), || {
            calls += 1;
            if calls < 3 {
                Err(err("rate_limited"))
            } else {
                Ok(())
            }
        });
        assert!(r.is_ok());
        assert_eq!(calls, 3);

        let mut calls = 0;
        let r: Result<(), _> = Client::retrying(Instant::now() + Duration::from_secs(30), || {
            calls += 1;
            Err(err("permission"))
        });
        assert_eq!(r.unwrap_err().code, "permission");
        assert_eq!(calls, 1);

        // A wait that would pass the deadline gives up instead.
        let r: Result<(), _> = Client::retrying(Instant::now(), || Err(err("server")));
        assert_eq!(r.unwrap_err().code, "server");
    }
}
