//! The cloud host (`--cloud`): run a script as an Open Cloud Luau Execution
//! task. See docs/design.md and docs/cloud-behavior.md.

pub mod api;
pub mod config;
pub mod wrapper;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::output::{
    EXIT_TIMEOUT, EXIT_TOOL_ERROR, EXIT_USAGE, Format, OutputFilter, Printer, StartInfo, note,
};
use crate::temp::TempDir;
use api::{ApiError, Client};

/// The API's maximum task timeout.
pub const MAX_TIMEOUT_SECS: u64 = 300;

pub struct CloudOptions {
    pub source: Vec<u8>,
    pub chunk_name: String,
    /// Run on this existing version instead of the latest.
    pub place_version: Option<u64>,
    pub flags: config::Flags,
    pub config: Option<PathBuf>,
    pub timeout: Duration,
    pub queue_timeout: Duration,
    pub poll_interval: Duration,
    pub format: Format,
    pub filter: OutputFilter,
    pub capture_print: bool,
    /// Load required ModuleScripts ourselves (opt-in; see README).
    pub trace_modules: bool,
    pub keep_temp: bool,
    /// From `--arg key=value`, for the script's context.
    pub args: BTreeMap<String, String>,
}

/// The settings and key every cloud command needs, or the exit code to use.
fn setup(
    config: Option<&std::path::Path>,
    flags: &config::Flags,
) -> Result<(config::Settings, Client), i32> {
    let settings = config::load(config, flags).map_err(|e| {
        note(&e);
        EXIT_USAGE
    })?;
    let key = std::env::var(&settings.api_key_env)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let Some(key) = key else {
        note(&format!(
            "set {} to an Open Cloud API key (or pass --api-key-env); see the README",
            settings.api_key_env
        ));
        return Err(EXIT_USAGE);
    };
    Ok((settings, Client::new(key)))
}

/// Run the script and return the process exit code to use.
pub fn run(opts: &CloudOptions) -> i32 {
    let secs = opts.timeout.as_secs();
    if secs == 0 || secs > MAX_TIMEOUT_SECS {
        note(&format!(
            "--timeout must be between 1 and {MAX_TIMEOUT_SECS} seconds under --cloud (got {secs})"
        ));
        return EXIT_USAGE;
    }
    let (settings, client) = match setup(opts.config.as_deref(), &opts.flags) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let context = crate::wrapper::ScriptContext {
        host: "cloud",
        args: &opts.args,
        universe_id: Some(settings.universe_id),
        place_id: Some(settings.place_id),
        // The wrapper reads it from game.PlaceVersion once the task runs.
        place_version: None,
    };
    let script = wrapper::generate(
        &opts.source,
        &opts.chunk_name,
        opts.capture_print,
        opts.trace_modules,
        &context.to_lua(),
    );
    if script.len() > wrapper::MAX_SCRIPT_BYTES {
        note(&format!(
            "the script is too large for a Luau Execution task ({} bytes with the wrapper and --arg values; the limit is {} bytes)",
            script.len(),
            wrapper::MAX_SCRIPT_BYTES
        ));
        return EXIT_USAGE;
    }
    let temp = if opts.keep_temp {
        match TempDir::create(true) {
            Ok(t) => Some(t),
            Err(e) => {
                note(&e);
                return EXIT_TOOL_ERROR;
            }
        }
    } else {
        None
    };
    let keep = |name: &str, contents: &str| {
        if let Some(t) = &temp {
            let _ = std::fs::write(t.path.join(name), contents);
        }
    };
    keep("task.luau", &script);

    let mut out = Printer::new(
        opts.format,
        opts.filter,
        StartInfo {
            host: "cloud",
            universe_id: Some(settings.universe_id),
            place_id: Some(settings.place_id),
            ..Default::default()
        },
    );
    let code = Run {
        opts,
        settings: &settings,
        client: &client,
        out: &mut out,
        started: Instant::now(),
        keep: &keep,
    }
    .go(&script);
    if let Some(t) = &temp {
        note(&format!("kept temp files in {}", t.path.display()));
    }
    code
}

struct Run<'a> {
    opts: &'a CloudOptions,
    settings: &'a config::Settings,
    client: &'a Client,
    out: &'a mut Printer,
    started: Instant,
    keep: &'a dyn Fn(&str, &str),
}

impl Run<'_> {
    fn status(&self, msg: &str) {
        if self.opts.filter == OutputFilter::Verbose && self.opts.format == Format::Text {
            note(msg);
        }
    }

    /// When the task should have finished: queueing allowance plus its timeout.
    fn deadline(&self) -> Instant {
        self.started + self.opts.queue_timeout + self.opts.timeout + Duration::from_secs(10)
    }

    fn fail(&mut self, e: &ApiError, what: &str) -> i32 {
        let mut msg = format!("{what}: {e}");
        match e.code {
            "auth" => msg.push_str(&format!(
                " (check the key in {}, and that it allows this machine's IP address)",
                self.settings.api_key_env
            )),
            "permission" => {
                msg.push_str(" (the key needs Luau execution sessions write on this experience)")
            }
            _ => {}
        }
        self.out.error(e.code, &msg);
        EXIT_TOOL_ERROR
    }

    fn go(&mut self, script: &str) -> i32 {
        let s = self.settings;
        let task = match self.client.create_task(
            s.universe_id,
            s.place_id,
            self.opts.place_version,
            script,
            self.opts.timeout.as_secs(),
            self.deadline(),
        ) {
            Ok(t) => t,
            Err(e) => return self.fail(&e, "couldn't create the task"),
        };
        let Some(path) = task["path"].as_str().map(str::to_string) else {
            self.out
                .error("http", &format!("the created task has no path: {task}"));
            return EXIT_TOOL_ERROR;
        };
        let ids = api::parse_task_path(&path);
        self.out.start.place_version = ids.version.or(self.opts.place_version);
        self.out.start.task_id = ids.task_id.clone();
        let task_id = ids.task_id.unwrap_or_else(|| path.clone());
        self.status(&format!(
            "created task {task_id} on version {}",
            self.out
                .start
                .place_version
                .map_or("?".into(), |v| v.to_string())
        ));

        let mut task = task;
        let mut last_state = String::new();
        loop {
            let state = task["state"].as_str().unwrap_or("").to_string();
            if state != last_state {
                self.status(&format!("task {}", state.to_lowercase()));
                last_state = state.clone();
            }
            if !matches!(
                state.as_str(),
                "QUEUED" | "PROCESSING" | "STATE_UNSPECIFIED" | ""
            ) {
                break;
            }
            if Instant::now() + self.opts.poll_interval >= self.deadline() {
                note(&format!(
                    "timed out after {}s waiting for task {task_id}; it keeps running until its own \
                     timeout, because tasks can't be cancelled",
                    self.started.elapsed().as_secs()
                ));
                return EXIT_TIMEOUT;
            }
            std::thread::sleep(self.opts.poll_interval);
            task = match self.client.get_task(&path, self.deadline()) {
                Ok(t) => t,
                Err(e) => return self.fail(&e, &format!("couldn't read task {task_id}")),
            };
        }
        (self.keep)("task.json", &redacted(&task));
        self.finished(&task, &path)
    }

    fn finished(&mut self, task: &Value, path: &str) -> i32 {
        let state = task["state"].as_str().unwrap_or("");
        if state == "COMPLETE" {
            match task["binaryOutputUri"].as_str() {
                Some(uri) => match self.client.download(uri, self.deadline()) {
                    Ok(bytes) => {
                        for line in String::from_utf8_lossy(&bytes).lines() {
                            self.out.line(line);
                        }
                    }
                    Err(e) => {
                        note(&format!(
                            "couldn't download the task's output ({e}); reading its logs instead"
                        ));
                        self.print_logs(path);
                    }
                },
                None => self.print_logs(path),
            }
            self.out.finish();
            if let Some(code) = self.out.exit_code() {
                return code;
            }
            // The events were lost, but the result also comes back as the return value.
            if let Some(code) = task["output"]["results"][0]["exitCode"].as_i64() {
                return code as i32;
            }
            self.out.error(
                "task_failed",
                "the task completed without reporting a result",
            );
            return EXIT_TOOL_ERROR;
        }

        // FAILED or CANCELLED: whatever the script printed is in the logs.
        self.print_logs(path);
        self.out.finish();
        let code = task["error"]["code"].as_str().unwrap_or("");
        let message = task["error"]["message"].as_str().unwrap_or("");
        if code == "DEADLINE_EXCEEDED" {
            note(&format!(
                "timed out after {}s (the task's timeout)",
                self.opts.timeout.as_secs()
            ));
            return EXIT_TIMEOUT;
        }
        if let Some(code) = self.out.exit_code() {
            return code;
        }
        let msg = if code.is_empty() {
            format!("the task ended as {state} without a result")
        } else {
            format!("the task failed without a result: {code}: {message}")
        };
        self.out.error("task_failed", &msg);
        EXIT_TOOL_ERROR
    }

    fn print_logs(&mut self, path: &str) {
        match self.client.logs(path, self.deadline()) {
            Ok(lines) => {
                for line in &lines {
                    self.out.line(line);
                }
            }
            Err(e) => note(&format!("couldn't read the task's logs: {e}")),
        }
    }
}

/// The task resource for `--keep-temp`, without its presigned URL or script.
fn redacted(task: &Value) -> String {
    let mut t = task.clone();
    if let Some(obj) = t.as_object_mut() {
        if obj.contains_key("binaryOutputUri") {
            obj.insert("binaryOutputUri".into(), "<redacted>".into());
        }
        obj.remove("script");
    }
    serde_json::to_string_pretty(&t).unwrap_or_default()
}

/// What a task can do, checked by `doctor --cloud`.
const DOCTOR_SCRIPT: &str = r#"
local RunService = game:GetService("RunService")
local m = Instance.new("ModuleScript")
m.Source = "return 42"
-- Tasks are created with binary output enabled, as for real runs, so the
-- result must come back in this shape.
return {
	BinaryOutput = buffer.create(0),
	ReturnValues = { {
		engine = version(),
		placeVersion = game.PlaceVersion,
		loadstring = pcall(function() return loadstring("return 1")() end),
		moduleScripts = (select(2, pcall(require, m))) == 42,
		isServer = RunService:IsServer(),
		isRunning = RunService:IsRunning(),
		plugin = plugin ~= nil,
	} },
}
"#;

pub struct DoctorOptions {
    pub flags: config::Flags,
    pub config: Option<PathBuf>,
}

/// `doctor --cloud`: check the settings and key, and run a trivial task.
pub fn doctor(opts: &DoctorOptions) -> i32 {
    let (settings, client) = match setup(opts.config.as_deref(), &opts.flags) {
        Ok(s) => s,
        Err(code) => {
            println!("FAILED: see the message above.");
            return code;
        }
    };
    if let Some(f) = &settings.file {
        println!("Config:   {}", f.display());
    }
    println!(
        "Place:    {} (universe {})",
        settings.place_id, settings.universe_id
    );
    println!("API key:  {} is set", settings.api_key_env);
    println!("Running a test task on the latest version...");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(180);
    let result = client
        .create_task(
            settings.universe_id,
            settings.place_id,
            None,
            DOCTOR_SCRIPT,
            60,
            deadline,
        )
        .and_then(|t| {
            let path = t["path"].as_str().unwrap_or_default().to_string();
            let mut t = t;
            while matches!(t["state"].as_str(), Some("QUEUED" | "PROCESSING")) {
                if Instant::now() >= deadline {
                    return Ok(t);
                }
                std::thread::sleep(Duration::from_secs(2));
                t = client.get_task(&path, deadline)?;
            }
            Ok(t)
        });
    let secs = started.elapsed().as_secs_f32();
    match result {
        Err(e) => {
            let hint = match e.code {
                "auth" => " Check the key, and that it allows this machine's IP address.",
                "permission" => {
                    " The key needs Luau execution sessions read and write on this experience."
                }
                "not_found" => " Check the universe and place IDs.",
                _ => "",
            };
            println!("FAILED: couldn't run a task: {e}.{hint}");
            EXIT_TOOL_ERROR
        }
        Ok(t) if t["state"] == "COMPLETE" => {
            let r = &t["output"]["results"][0];
            println!("Engine:   {}", r["engine"].as_str().unwrap_or("?"));
            println!("Version:  {}", r["placeVersion"]);
            println!(
                "Inside a task: loadstring {}, ModuleScripts {}, server {}, running {}, plugin {}",
                yes_no(&r["loadstring"]),
                yes_no(&r["moduleScripts"]),
                yes_no(&r["isServer"]),
                yes_no(&r["isRunning"]),
                yes_no(&r["plugin"]),
            );
            if r["moduleScripts"] != true {
                println!(
                    "FAILED: the task can't require a ModuleScript it creates; --cloud relies on that \
                     (see docs/cloud-behavior.md)."
                );
                return EXIT_TOOL_ERROR;
            }
            println!("OK: ran a task in {secs:.1}s.");
            0
        }
        Ok(t) => {
            println!(
                "FAILED: the test task ended as {} after {secs:.1}s: {}",
                t["state"].as_str().unwrap_or("?"),
                t["error"]
            );
            EXIT_TOOL_ERROR
        }
    }
}

fn yes_no(v: &Value) -> &'static str {
    if v == true { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_the_presigned_url_and_script() {
        let t = serde_json::json!({
            "path": "p", "binaryOutputUri": "https://secret", "script": "print(1)", "state": "COMPLETE"
        });
        let r = redacted(&t);
        assert!(!r.contains("https://secret"));
        assert!(!r.contains("print(1)"));
        assert!(r.contains("COMPLETE"));
    }
}
