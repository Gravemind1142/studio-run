//! studio-run: run Luau scripts in Roblox Studio without showing Studio's
//! window. See README.md.

mod events;
mod locate;
mod logtail;
mod run;
mod sandbox;
mod wrapper;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};

use run::{Format, OutputFilter, RunOptions, Target, note};

#[derive(Parser)]
#[command(
    version,
    about = "Run Luau scripts in Roblox Studio without showing its window"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a Luau script in hidden Studio and print its output.
    Run(RunArgs),
    /// Write Roblox's API dump as JSON.
    ApiDump(ApiDumpArgs),
    /// Check that Studio can be found and can run a script.
    Doctor(StudioArg),
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

#[derive(Args)]
struct RunArgs {
    /// The .lua/.luau script to run.
    script: PathBuf,

    /// Open this .rbxl/.rbxlx place file first (default: an empty baseplate).
    #[arg(long, conflicts_with = "place_id")]
    place: Option<PathBuf>,

    /// Open this published place first (needs --universe-id).
    #[arg(long, requires = "universe_id")]
    place_id: Option<u64>,

    /// Universe of --place-id.
    #[arg(long, requires = "place_id")]
    universe_id: Option<u64>,

    /// Seconds before Studio is killed and the run fails (exit code 124).
    #[arg(long, default_value_t = 300)]
    timeout: u64,

    /// Seconds to let Studio quit on its own after the script finishes.
    #[arg(long, default_value_t = 5)]
    grace: u64,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Which output to show in text format.
    #[arg(long, value_enum, default_value_t = OutputFilter::Normal)]
    output: OutputFilter,

    /// Don't replace print/warn in the script's environment. Its output is
    /// then reported as `other`, like any other script's.
    #[arg(long)]
    no_capture: bool,

    /// Load required ModuleScripts through studio-run so their output and
    /// errors are attributed to them. Not the real `require`: don't use with
    /// test runners or other code that has its own module loader.
    #[arg(long)]
    trace_modules: bool,

    /// Show Studio's window on your desktop (for debugging).
    #[arg(long)]
    visible: bool,

    /// Keep the generated wrapper and output file, and print where they are.
    #[arg(long)]
    keep_temp: bool,

    #[command(flatten)]
    studio: StudioArg,

    /// Extra arguments passed to Studio as-is.
    #[arg(last = true)]
    studio_args: Vec<String>,
}

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
        Command::Run(a) => cmd_run(a),
        Command::ApiDump(a) => cmd_api_dump(a),
        Command::Doctor(a) => cmd_doctor(a),
        Command::Raw(a) => cmd_raw(a),
    };
    std::process::exit(code);
}

fn studio_or_exit(arg: StudioArg) -> Result<PathBuf, i32> {
    locate::studio_exe(arg.studio).map_err(|e| {
        note(&e);
        run::EXIT_TOOL_ERROR
    })
}

fn cmd_run(a: RunArgs) -> i32 {
    let exe = match studio_or_exit(a.studio) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let source = match std::fs::read(&a.script) {
        Ok(s) => s,
        Err(e) => {
            note(&format!("couldn't read {}: {e}", a.script.display()));
            return 2;
        }
    };
    let chunk_name = a
        .script
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script.luau".into());
    let target = match (a.place, a.place_id, a.universe_id) {
        (Some(p), _, _) => Target::PlaceFile(p),
        (None, Some(place_id), Some(universe_id)) => Target::Published {
            place_id,
            universe_id,
        },
        _ => Target::Baseplate,
    };
    run::run(&RunOptions {
        source,
        chunk_name,
        target,
        timeout: Duration::from_secs(a.timeout),
        grace: Duration::from_secs(a.grace),
        format: a.format,
        filter: a.output,
        capture_print: !a.no_capture,
        trace_modules: a.trace_modules,
        visible: a.visible,
        keep_temp: a.keep_temp,
        studio_exe: exe,
        studio_args: a.studio_args,
    })
}

fn cmd_api_dump(a: ApiDumpArgs) -> i32 {
    let exe = match studio_or_exit(a.studio) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let out = match std::path::absolute(&a.out) {
        Ok(p) => p,
        Err(e) => {
            note(&format!("bad output path: {e}"));
            return 2;
        }
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
            return run::EXIT_TOOL_ERROR;
        }
    };
    if !studio.wait((a.timeout * 1000) as u32) {
        note(&format!("timed out after {}s", a.timeout));
        studio.kill();
        return run::EXIT_TIMEOUT;
    }
    match std::fs::metadata(&out) {
        Ok(m) if m.len() > 0 => {
            note(&format!("wrote {} ({} bytes)", out.display(), m.len()));
            0
        }
        _ => {
            note(&format!("Studio exited without writing {}", out.display()));
            run::EXIT_TOOL_ERROR
        }
    }
}

fn cmd_doctor(a: StudioArg) -> i32 {
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
fn cmd_raw(a: RawArgs) -> i32 {
    let exe = match studio_or_exit(a.studio) {
        Ok(p) => p,
        Err(c) => return c,
    };
    let started = Instant::now();
    let studio = match sandbox::Studio::launch(&exe, &a.studio_args, !a.visible) {
        Ok(s) => s,
        Err(e) => {
            note(&e);
            return run::EXIT_TOOL_ERROR;
        }
    };
    let secs = || started.elapsed().as_secs_f32();
    if !studio.wait((a.timeout * 1000) as u32) {
        note(&format!("Studio was still running after {:.1}s", secs()));
        for w in studio.windows() {
            note(&format!("open window: [{}] {}", w.class, w.title));
        }
        studio.kill();
        return run::EXIT_TIMEOUT;
    }
    let code = studio.exit_code().unwrap_or(u32::MAX);
    note(&format!(
        "Studio exited with code {code} after {:.1}s",
        secs()
    ));
    code as i32
}
