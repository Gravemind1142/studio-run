//! studio-run: run Luau scripts in hidden Roblox Studio (Windows and macOS), or as
//! Open Cloud Luau Execution tasks (`--cloud`, anywhere). See README.md.

mod binary;
mod cloud;
mod events;
mod output;
mod temp;
mod wrapper;

#[cfg(any(windows, target_os = "macos"))]
mod locate;
#[cfg(any(windows, target_os = "macos"))]
mod logtail;
#[cfg(any(windows, target_os = "macos"))]
mod run;
#[cfg(any(windows, target_os = "macos"))]
mod sandbox;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use binary::Input;
use output::{EXIT_USAGE, Format, OutputFilter, note};

#[derive(Parser)]
#[command(
    version,
    about = "Run Luau scripts in hidden Roblox Studio, or as Open Cloud Luau Execution tasks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a Luau script in hidden Studio (or with --cloud, as an Open Cloud
    /// Luau Execution task) and print its output.
    Run(Box<RunArgs>),
    /// Write Roblox's API dump as JSON. Windows and macOS only.
    ApiDump(ApiDumpArgs),
    /// Check that Studio can be found and can run a script (or with --cloud,
    /// that a task can run).
    Doctor(DoctorArgs),
    /// Launch hidden Studio with your own arguments and no wrapper. Used by
    /// the behaviour checks in tests/behavior.rs.
    #[command(hide = true)]
    Raw(RawArgs),
}

#[derive(Args)]
struct StudioArg {
    /// Path to the Studio executable, or on macOS the .app bundle
    /// (default: find the installed Studio).
    #[arg(long, env = "STUDIO_EXE", global = true)]
    studio: Option<PathBuf>,
}

/// Options shared by `run --cloud` and `doctor --cloud`.
#[derive(Args)]
struct CloudArgs {
    /// Run as an Open Cloud Luau Execution task instead of in local Studio.
    /// Also STUDIO_RUN_HOST=cloud.
    #[arg(long, visible_alias = "rocale")]
    cloud: bool,

    /// Environment variable holding the Open Cloud API key [default: ROBLOX_API_KEY].
    #[arg(long, value_name = "NAME")]
    api_key_env: Option<String>,

    /// Config file [default: studio-run.toml in this directory or a parent].
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl CloudArgs {
    fn enabled(&self) -> bool {
        self.cloud
            || std::env::var("STUDIO_RUN_HOST").is_ok_and(|h| h.eq_ignore_ascii_case("cloud"))
    }
}

#[derive(Args)]
struct RunArgs {
    /// The .lua/.luau script to run.
    script: PathBuf,

    /// Place file to run in (default: an empty baseplate). Local only.
    #[arg(long)]
    place: Option<PathBuf>,

    /// Published place to run in (needs --universe-id). With --cloud, the
    /// place to run on, if not set in studio-run.toml.
    #[arg(long)]
    place_id: Option<u64>,

    /// Universe of --place-id.
    #[arg(long)]
    universe_id: Option<u64>,

    /// Seconds before the run fails (exit code 124). At most 300 with --cloud.
    #[arg(long, default_value_t = 300)]
    timeout: u64,

    /// Seconds to let Studio quit on its own after the script finishes [default: 5].
    /// Ignored with --cloud.
    #[arg(long)]
    grace: Option<u64>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Which output to show in text format.
    #[arg(long, value_enum, default_value_t = OutputFilter::Normal)]
    output: OutputFilter,

    /// Pass a string to the script, as `ctx.args[key]` (`local ctx = ...`).
    /// Repeatable.
    #[arg(long = "arg", value_name = "KEY=VALUE", value_parser = parse_arg)]
    args: Vec<(String, String)>,

    /// Don't replace print/warn in the script's environment. Its output is
    /// then reported as `other`, like any other script's.
    #[arg(long)]
    no_capture: bool,

    /// Load required ModuleScripts through studio-run so their output and
    /// errors are attributed to them. Not the real `require`: don't use with
    /// test runners or other code that has its own module loader.
    #[arg(long)]
    trace_modules: bool,

    /// Show Studio's window on your desktop (for debugging). Local only.
    #[arg(long)]
    visible: bool,

    /// Pass FILE to the script as the buffer `ctx.inputs[NAME]` (repeatable).
    /// At most 100 MiB in total.
    #[arg(long = "input", value_name = "NAME=FILE")]
    inputs: Vec<String>,

    /// Write the buffer the script passes to `ctx.setBinaryOutput` to FILE (an
    /// empty file if it passes none). At most 8 MiB locally, 256 MiB with --cloud.
    #[arg(long, value_name = "FILE")]
    binary_output: Option<PathBuf>,

    /// Keep the generated wrapper and output (or task script and task), and
    /// print where they are.
    #[arg(long)]
    keep_temp: bool,

    #[command(flatten)]
    cloud: CloudArgs,

    /// --cloud: run on this existing place version instead of the latest.
    #[arg(long, value_name = "N")]
    place_version: Option<u64>,

    /// --cloud: seconds allowed for the task to start, on top of --timeout.
    #[arg(long, default_value_t = 120, value_name = "S")]
    queue_timeout: u64,

    /// --cloud: seconds between task status checks.
    #[arg(long, default_value_t = 2, value_name = "S")]
    poll_interval: u64,

    #[command(flatten)]
    studio: StudioArg,

    /// Extra arguments passed to Studio as-is. Local only.
    #[arg(last = true)]
    studio_args: Vec<String>,
}

// Without a local host (Linux), these are parsed but never read.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Args)]
struct ApiDumpArgs {
    /// Where to write the JSON.
    out: PathBuf,

    /// Which dump to write.
    #[arg(long, value_enum, default_value_t = ApiKind::V2)]
    kind: ApiKind,

    /// Seconds before giving up.
    #[arg(long, default_value_t = 120)]
    timeout: u64,

    #[command(flatten)]
    studio: StudioArg,
}

#[derive(Args)]
struct DoctorArgs {
    #[command(flatten)]
    cloud: CloudArgs,

    /// --cloud: the place to run a test task on, if not set in studio-run.toml.
    #[arg(long)]
    place_id: Option<u64>,

    /// --cloud: universe of --place-id.
    #[arg(long)]
    universe_id: Option<u64>,

    #[command(flatten)]
    studio: StudioArg,
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Args)]
struct RawArgs {
    /// Seconds before Studio is killed (exit code 124).
    #[arg(long, default_value_t = 120)]
    timeout: u64,

    /// Show Studio's window on your desktop.
    #[arg(long)]
    visible: bool,

    #[command(flatten)]
    studio: StudioArg,

    /// Arguments passed to Studio as-is.
    #[arg(last = true)]
    studio_args: Vec<String>,
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Clone, Copy, ValueEnum)]
enum ApiKind {
    /// Full API, V2 schema (`--apiV2`).
    V2,
    /// Full API (`--fullApi`).
    Full,
    /// Luau scripting API (`--api`).
    Luau,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Run(a) => cmd_run(*a),
        Command::ApiDump(a) => local::api_dump(a),
        Command::Doctor(a) if a.cloud.enabled() => cmd_doctor_cloud(a),
        Command::Doctor(a) => local::doctor(a.studio),
        Command::Raw(a) => local::raw(a),
    };
    std::process::exit(code);
}

fn usage(msg: &str) -> i32 {
    note(msg);
    EXIT_USAGE
}

/// `--arg key=value`: split at the first `=`, so values may contain `=`.
fn parse_arg(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some(("", _)) => Err("the key is empty; expected KEY=VALUE".into()),
        Some((k, v)) => Ok((k.to_string(), v.to_string())),
        None => Err("expected KEY=VALUE".into()),
    }
}

/// The `--arg` values by key, or a usage error if a key repeats.
fn script_args(pairs: Vec<(String, String)>) -> Result<BTreeMap<String, String>, String> {
    let mut args = BTreeMap::new();
    for (k, v) in pairs {
        if args.contains_key(&k) {
            return Err(format!("--arg {k} is given more than once"));
        }
        args.insert(k, v);
    }
    Ok(args)
}

fn cmd_run(mut a: RunArgs) -> i32 {
    let source = match std::fs::read(&a.script) {
        Ok(s) => s,
        Err(e) => return usage(&format!("couldn't read {}: {e}", a.script.display())),
    };
    let chunk_name = a
        .script
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script.luau".into());
    let args = match script_args(std::mem::take(&mut a.args)) {
        Ok(args) => args,
        Err(e) => return usage(&e),
    };
    let inputs = match binary::read_inputs(&a.inputs) {
        Ok(i) => i,
        Err(e) => return usage(&e),
    };
    if a.cloud.enabled() {
        cmd_run_cloud(a, source, chunk_name, args, inputs)
    } else {
        local::run(a, source, chunk_name, args, inputs)
    }
}

fn cmd_run_cloud(
    a: RunArgs,
    source: Vec<u8>,
    chunk_name: String,
    args: BTreeMap<String, String>,
    inputs: Vec<Input>,
) -> i32 {
    if a.visible {
        return usage("--visible needs local Studio; it can't be used with --cloud");
    }
    if !a.studio_args.is_empty() {
        return usage("extra Studio arguments (after --) can't be used with --cloud");
    }
    if a.place.is_some() {
        // Uploading the file would add a version to the place for every run.
        return usage(
            "--place isn't supported with --cloud; publish the place yourself, then run on \
             its latest version or pick one with --place-version",
        );
    }
    if a.grace.is_some() {
        note("--grace is ignored with --cloud");
    }
    if a.studio.studio.is_some() && std::env::var_os("STUDIO_EXE").is_none() {
        note("--studio is ignored with --cloud");
    }
    if a.poll_interval == 0 {
        return usage("--poll-interval must be at least 1");
    }
    cloud::run(&cloud::CloudOptions {
        source,
        chunk_name,
        place_version: a.place_version,
        flags: cloud::config::Flags {
            universe_id: a.universe_id,
            place_id: a.place_id,
            api_key_env: a.cloud.api_key_env,
        },
        config: a.cloud.config,
        timeout: Duration::from_secs(a.timeout),
        queue_timeout: Duration::from_secs(a.queue_timeout),
        poll_interval: Duration::from_secs(a.poll_interval),
        format: a.format,
        filter: a.output,
        capture_print: !a.no_capture,
        trace_modules: a.trace_modules,
        keep_temp: a.keep_temp,
        args,
        inputs,
        binary_output: a.binary_output,
    })
}

fn cmd_doctor_cloud(a: DoctorArgs) -> i32 {
    cloud::doctor(&cloud::DoctorOptions {
        flags: cloud::config::Flags {
            universe_id: a.universe_id,
            place_id: a.place_id,
            api_key_env: a.cloud.api_key_env,
        },
        config: a.cloud.config,
    })
}

/// The local Studio host: Windows and macOS.
#[cfg(any(windows, target_os = "macos"))]
mod local {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{ApiDumpArgs, ApiKind, Input, RawArgs, RunArgs, StudioArg, usage};
    use crate::output::{EXIT_TIMEOUT, EXIT_TOOL_ERROR, Format, OutputFilter, note};
    use crate::run::{self, RunOptions, Target};
    use crate::{locate, sandbox};

    fn studio_or_exit(arg: StudioArg) -> Result<PathBuf, i32> {
        locate::studio_exe(arg.studio).map_err(|e| {
            note(&e);
            EXIT_TOOL_ERROR
        })
    }

    pub fn run(
        a: RunArgs,
        source: Vec<u8>,
        chunk_name: String,
        args: BTreeMap<String, String>,
        inputs: Vec<Input>,
    ) -> i32 {
        if a.place_version.is_some() {
            return usage("--place-version needs --cloud");
        }
        let target = match (a.place, a.place_id, a.universe_id) {
            (Some(_), Some(_), _) => {
                return usage("--place and --place-id can't be used together without --cloud");
            }
            (Some(p), None, _) => Target::PlaceFile(p),
            (None, Some(place_id), Some(universe_id)) => Target::Published {
                place_id,
                universe_id,
            },
            (None, Some(_), None) | (None, None, Some(_)) => {
                return usage("--place-id and --universe-id must be given together");
            }
            (None, None, None) => Target::Baseplate,
        };
        let exe = match studio_or_exit(a.studio) {
            Ok(p) => p,
            Err(c) => return c,
        };
        run::run(&RunOptions {
            source,
            chunk_name,
            target,
            timeout: Duration::from_secs(a.timeout),
            grace: Duration::from_secs(a.grace.unwrap_or(5)),
            format: a.format,
            filter: a.output,
            capture_print: !a.no_capture,
            trace_modules: a.trace_modules,
            visible: a.visible,
            keep_temp: a.keep_temp,
            studio_exe: exe,
            studio_args: a.studio_args,
            args,
            inputs,
            binary_output: a.binary_output,
        })
    }

    pub fn api_dump(a: ApiDumpArgs) -> i32 {
        let exe = match studio_or_exit(a.studio) {
            Ok(p) => p,
            Err(c) => return c,
        };
        let out = match std::path::absolute(&a.out) {
            Ok(p) => p,
            Err(e) => return usage(&format!("bad output path: {e}")),
        };
        let _ = std::fs::remove_file(&out);
        let flag = match a.kind {
            ApiKind::V2 => "--apiV2",
            ApiKind::Full => "--fullApi",
            ApiKind::Luau => "--api",
        };
        let studio = match sandbox::Studio::launch(
            &exe,
            &[flag.into(), out.to_string_lossy().into_owned()],
            true,
        ) {
            Ok(s) => s,
            Err(e) => {
                note(&e);
                return EXIT_TOOL_ERROR;
            }
        };
        if !studio.wait((a.timeout * 1000) as u32) {
            note(&format!("timed out after {}s", a.timeout));
            studio.kill();
            return EXIT_TIMEOUT;
        }
        match std::fs::metadata(&out) {
            Ok(m) if m.len() > 0 => {
                note(&format!("wrote {} ({} bytes)", out.display(), m.len()));
                0
            }
            _ => {
                note(&format!("Studio exited without writing {}", out.display()));
                EXIT_TOOL_ERROR
            }
        }
    }

    pub fn doctor(a: StudioArg) -> i32 {
        let exe = match studio_or_exit(a) {
            Ok(p) => p,
            Err(c) => return c,
        };
        println!("Studio:   {}", exe.display());
        println!("Running a test script in hidden Studio...");
        let started = Instant::now();
        let code = run::run(&RunOptions {
            source: b"print(\"studio-run doctor: script ran\")\nreturn true\n".to_vec(),
            chunk_name: "doctor.luau".into(),
            target: Target::Baseplate,
            timeout: Duration::from_secs(120),
            grace: Duration::from_secs(5),
            format: Format::Text,
            filter: OutputFilter::Script,
            capture_print: true,
            trace_modules: false,
            visible: false,
            keep_temp: false,
            studio_exe: exe,
            studio_args: vec![],
            args: BTreeMap::new(),
            inputs: vec![],
            binary_output: None,
        });
        let secs = started.elapsed().as_secs_f32();
        if code == 0 {
            println!("OK: Studio ran a script without showing a window ({secs:.1}s).");
        } else {
            println!("FAILED (exit code {code}) after {secs:.1}s; see the messages above.");
        }
        code
    }

    /// Exit code: Studio's own, or 124 if it was still running at the timeout.
    pub fn raw(a: RawArgs) -> i32 {
        let exe = match studio_or_exit(a.studio) {
            Ok(p) => p,
            Err(c) => return c,
        };
        let started = Instant::now();
        let studio = match sandbox::Studio::launch(&exe, &a.studio_args, !a.visible) {
            Ok(s) => s,
            Err(e) => {
                note(&e);
                return EXIT_TOOL_ERROR;
            }
        };
        let secs = || started.elapsed().as_secs_f32();
        if !studio.wait((a.timeout * 1000) as u32) {
            note(&format!("Studio was still running after {:.1}s", secs()));
            for w in studio.windows() {
                note(&format!("open window: [{}] {}", w.class, w.title));
            }
            studio.kill();
            return EXIT_TIMEOUT;
        }
        let code = studio.exit_code().unwrap_or(u32::MAX);
        note(&format!(
            "Studio exited with code {code} after {:.1}s",
            secs()
        ));
        code as i32
    }
}

/// Elsewhere there's no Studio: everything but --cloud is a usage error.
#[cfg(not(any(windows, target_os = "macos")))]
mod local {
    use super::{ApiDumpArgs, Input, RawArgs, RunArgs, StudioArg, usage};

    const MSG: &str = "local Studio runs need Windows or macOS; use --cloud";

    pub fn run(
        _: RunArgs,
        _: Vec<u8>,
        _: String,
        _: std::collections::BTreeMap<String, String>,
        _: Vec<Input>,
    ) -> i32 {
        usage(MSG)
    }

    pub fn api_dump(_: ApiDumpArgs) -> i32 {
        usage("api-dump needs Studio, which needs Windows or macOS")
    }

    pub fn doctor(_: StudioArg) -> i32 {
        usage(MSG)
    }

    pub fn raw(_: RawArgs) -> i32 {
        usage("raw needs Studio, which needs Windows or macOS")
    }
}
