//! The local host: wrap a script, run it in hidden Studio, stream its events.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::binary::{self, Input};
use crate::logtail::LogTail;
use crate::output::{
    EXIT_TIMEOUT, EXIT_TOOL_ERROR, Format, OutputFilter, Printer, StartInfo, note,
};
use crate::sandbox::Studio;
use crate::temp::TempDir;
use crate::wrapper::{self, WrapperOptions};

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
    /// From `--input`, embedded in the wrapper.
    pub inputs: Vec<Input>,
    /// `--binary-output`: where to write the script's binary output.
    pub binary_output: Option<PathBuf>,
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
    let dir = TempDir::create(opts.keep_temp)?;
    let id = dir.id.clone();

    let wrapper_path = dir.path.join("wrapper.luau");
    let output_path = dir.path.join("output.log");
    let source = wrapper::generate(
        &opts.source,
        &opts.inputs,
        &WrapperOptions {
            chunk_name: opts.chunk_name.clone(),
            capture_print: opts.capture_print,
            trace_modules: opts.trace_modules,
            binary_output: opts.binary_output.is_some(),
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
    let mut start = StartInfo {
        host: "local",
        ..Default::default()
    };
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
            start.place_id = Some(*place_id);
            start.universe_id = Some(*universe_id);
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

    let mut out = Printer::new(opts.format, opts.filter, start);
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
        note(&format!("kept temp files in {}", dir.path.display()));
    }
    if let (Some(path), Some(_)) = (&opts.binary_output, &out.result) {
        // A script that never called ctx.setBinaryOutput leaves an empty file.
        binary::write_output(path, out.binary.as_deref().unwrap_or_default())?;
    }
    if let Some(code) = out.exit_code() {
        if killed_after_result && opts.filter == OutputFilter::Verbose {
            note("Studio didn't quit after the script finished; stopped it");
        }
        return Ok(code);
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
        None => note(&format!(
            "couldn't find this run's Studio log in {}",
            tail.dir().display()
        )),
    }
}

fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
