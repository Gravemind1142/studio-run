//! `studio-run run`: wrap a script, run it in hidden Studio, stream its events.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::events::{self, Assembler, Event};
use crate::logtail::LogTail;
use crate::sandbox::Studio;
use crate::wrapper::{self, WrapperOptions};

/// Exit code when the run timed out.
pub const EXIT_TIMEOUT: i32 = 124;
/// Exit code when Studio couldn't run the script or never reported a result.
pub const EXIT_TOOL_ERROR: i32 = 125;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Output as text, like Studio's Output window.
    Text,
    /// One JSON event per line on stdout.
    Jsonl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFilter {
    /// Only output from the script itself.
    Script,
    /// The script's output, plus print/warn/error from anything else.
    Normal,
    /// Everything, including Studio's info messages.
    Verbose,
}

pub enum Target {
    Baseplate,
    PlaceFile(PathBuf),
    Published { place_id: u64, universe_id: u64 },
}

pub struct RunOptions {
    pub source: Vec<u8>,
    pub chunk_name: String,
    pub target: Target,
    pub timeout: Duration,
    pub grace: Duration,
    pub format: Format,
    pub filter: OutputFilter,
    pub capture_print: bool,
    pub trace_modules: bool,
    pub visible: bool,
    pub keep_temp: bool,
    pub studio_exe: PathBuf,
    pub studio_args: Vec<String>,
}

/// Run the script and return the process exit code to use.
pub fn run(opts: &RunOptions) -> i32 {
    match run_inner(opts) {
        Ok(code) => code,
        Err(e) => {
            note(&e);
            EXIT_TOOL_ERROR
        }
    }
}

fn run_inner(opts: &RunOptions) -> Result<i32, String> {
    let id = run_id();
    let dir = std::env::temp_dir().join("studio-run").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let _cleanup = TempDir {
        path: dir.clone(),
        keep: opts.keep_temp,
    };

    let wrapper_path = dir.join("wrapper.luau");
    let output_path = dir.join("output.log");
    let source = wrapper::generate(
        &opts.source,
        &WrapperOptions {
            chunk_name: opts.chunk_name.clone(),
            capture_print: opts.capture_print,
            trace_modules: opts.trace_modules,
        },
    );
    std::fs::write(&wrapper_path, source).map_err(|e| format!("couldn't write wrapper: {e}"))?;

    let mut args: Vec<String> = vec![
        "--task".into(),
        "RunScript".into(),
        "--runScriptFile".into(),
        path_arg(&wrapper_path),
        "--outputFile".into(),
        path_arg(&output_path),
        "--quitAfterExecution".into(),
    ];
    match &opts.target {
        Target::Baseplate => {}
        Target::PlaceFile(p) => {
            let abs = std::path::absolute(p).map_err(|e| format!("bad place path: {e}"))?;
            if !abs.is_file() {
                return Err(format!("place file not found: {}", abs.display()));
            }
            args.extend(["--localPlaceFile".into(), path_arg(&abs)]);
        }
        Target::Published {
            place_id,
            universe_id,
        } => {
            args.extend([
                "--placeId".into(),
                place_id.to_string(),
                "--universeId".into(),
                universe_id.to_string(),
            ]);
        }
    }
    args.extend(opts.studio_args.iter().cloned());

    // The run id is in the wrapper path, hence in the command line Studio logs.
    let mut tail = LogTail::new(&id);
    let studio = Studio::launch(&opts.studio_exe, &args, !opts.visible)?;
    let started = Instant::now();

    let mut out = Printer {
        format: opts.format,
        filter: opts.filter,
        asm: Assembler::default(),
        result: None,
    };
    let mut result_at: Option<Instant> = None;
    let mut timed_out = false;
    let mut killed_after_result = false;

    loop {
        let exited = studio.wait(100);
        for line in tail.poll() {
            out.line(&line);
        }
        if out.result.is_some() && result_at.is_none() {
            result_at = Some(Instant::now());
        }
        if exited {
            break;
        }
        // Studio sometimes doesn't quit after the script; don't wait on it.
        if result_at.is_some_and(|t| t.elapsed() >= opts.grace) {
            killed_after_result = true;
            studio.kill();
            break;
        }
        if started.elapsed() >= opts.timeout {
            timed_out = true;
            report_timeout(opts, &studio, &tail);
            studio.kill();
            break;
        }
    }

    for line in tail.poll() {
        out.line(&line);
    }
    // Fallback: the output file is complete once Studio exits (or is killed
    // after the script finished). Already-seen events are skipped.
    if let Ok(text) = std::fs::read(&output_path) {
        for line in String::from_utf8_lossy(&text).lines() {
            out.line(line);
        }
    }
    out.finish();

    if opts.keep_temp {
        note(&format!("kept temp files in {}", dir.display()));
    }
    if let Some(code) = out.result.as_ref().and_then(|r| r.exit_code) {
        if killed_after_result && opts.filter == OutputFilter::Verbose {
            note("Studio didn't quit after the script finished; stopped it");
        }
        return Ok(code as i32);
    }
    if timed_out {
        return Ok(EXIT_TIMEOUT);
    }
    let code = studio
        .exit_code()
        .map(|c| format!("{c:#x}"))
        .unwrap_or("unknown".into());
    note(&format!(
        "Studio exited (code {code}) without the script reporting a result"
    ));
    hints(&tail);
    Ok(EXIT_TOOL_ERROR)
}

fn report_timeout(opts: &RunOptions, studio: &Studio, tail: &LogTail) {
    note(&format!("timed out after {}s", opts.timeout.as_secs()));
    let windows = studio.windows();
    if !windows.is_empty() {
        let titles: Vec<String> = windows
            .iter()
            .map(|w| format!("{:?} ({})", w.title, w.class))
            .collect();
        note(&format!(
            "Studio windows open at timeout: {}",
            titles.join(", ")
        ));
    }
    hints(tail);
}

fn hints(tail: &LogTail) {
    if tail.authenticated == Some(false) {
        note("Studio doesn't appear to be logged in; open Studio normally and log in once");
    }
    match tail.path() {
        Some(p) => note(&format!("Studio log: {}", p.display())),
        None => note("couldn't find this run's Studio log in %LOCALAPPDATA%\\Roblox\\logs"),
    }
}

struct Printer {
    format: Format,
    filter: OutputFilter,
    asm: Assembler,
    result: Option<Event>,
}

impl Printer {
    fn line(&mut self, line: &str) {
        match events::parse_line(line) {
            None => {}
            Some(Ok(ev)) => {
                if let Some(ev) = self.asm.push(ev) {
                    self.event(ev);
                }
            }
            Some(Err(e)) => note(&format!("ignoring malformed event: {e}")),
        }
    }

    fn finish(&mut self) {
        if let Some(ev) = self.asm.finish() {
            self.event(ev);
        }
    }

    fn event(&mut self, ev: Event) {
        // `result` ends the run; anything after it is Studio shutting down.
        if self.result.is_some() {
            return;
        }
        match self.format {
            Format::Jsonl => {
                let mut stdout = std::io::stdout().lock();
                let _ = writeln!(stdout, "{}", serde_json::to_string(&ev).unwrap_or_default());
            }
            Format::Text => self.text(&ev),
        }
        if ev.kind == "result" {
            self.result = Some(ev);
        }
    }

    fn text(&self, ev: &Event) {
        if ev.kind != "output" {
            return;
        }
        let from_script = ev.source.as_deref() == Some("script");
        let level = ev.level.as_deref().unwrap_or("print");
        let show = match self.filter {
            OutputFilter::Script => from_script,
            OutputFilter::Normal => from_script || level != "info",
            OutputFilter::Verbose => true,
        };
        if !show {
            return;
        }
        let msg = ev.message.as_deref().unwrap_or("");
        match level {
            "warn" => eprintln!("warning: {msg}"),
            "error" => eprintln!("error: {msg}"),
            _ => println!("{msg}"),
        }
    }
}

/// Tool messages go to stderr, prefixed, so stdout stays the script's.
pub fn note(msg: &str) {
    eprintln!("studio-run: {msg}");
}

fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("run-{}-{:x}", std::process::id(), nanos)
}

struct TempDir {
    path: PathBuf,
    keep: bool,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
