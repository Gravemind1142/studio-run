# How Roblox Studio's CLI behaves

Studio's [command-line options](https://create.roblox.com/docs/studio/cli)
are only partly documented, and some of the documentation is wrong for
Windows. This file records what we found by running Studio, how we found it,
and which check in [tests/behavior.rs](../tests/behavior.rs) re-tests it.
[design.md](design.md) explains how studio-run is built around these findings.

Last checked: Studio **0.748.0.7481526** on Windows 11, 2026-11-22.
On macOS: Studio **0.748.0.7481526**, 2026-11-22 (see [macOS](#macos)).

## Re-checking after a Studio update

```powershell
cargo test --test behavior -- --ignored --nocapture
```

This takes about 90 seconds and runs up to three hidden Studio instances at
once. Each test checks one finding. A failure means Studio has changed:
update the finding here, update "Last checked", and change studio-run if it
relied on the old behaviour. Then run the end-to-end tests
(`cargo test --test studio -- --ignored`) to see whether studio-run still works.

## How we investigated

**Hide Studio first.** All experiments ran Studio on a private desktop (see
[design.md](design.md#hiding-studio)), so Studio could be launched dozens of
times without windows appearing. `--visible` on `run` and `raw` shows it when
you need to see what's on screen.

**Run without the wrapper to see Studio's own behaviour.**
`studio-run raw -- <Studio arguments>` launches hidden Studio with exactly the
arguments you give, waits, and reports Studio's exit code (124 if it was still
running at `--timeout`, listing its open windows). The `raw_*` probes in
`tests/probes/` are run this way, with `--outputFile` and Studio's own log to
see what happened.

**Run through the wrapper to see what a script can do.** The other probes run
with `studio-run run <probe> --no-capture`, so the probe gets the real
`print`, and report one JSON line starting with `@@probe@@`.

**Read Studio's log.** Studio writes a log per process to
`%LOCALAPPDATA%\Roblox\logs\<version>_<time>_Studio_<id>_last.log`. Its
header includes the full command line, which identifies the run. Script
output appears as `[FLog::CreatorOutput]`, `CreatorWarning` and
`CreatorError` lines.

**Split strings in probes.** Studio echoes the script's source into the
output, so a probe prints `"PROBE" .. "-early"` rather than `"PROBE-early"`.
Only the real output then contains `PROBE-early`.

**Repeat flaky things.** Quitting is unreliable, so checks that need Studio to
exit retry up to three times, and quit reliability itself is measured rather
than asserted.

## Findings

### Launching

| Finding | How we know | Check |
|---|---|---|
| A private desktop hides Studio completely. It renders and runs normally, no window appears, focus isn't taken, and several instances can run at once on separate desktops. | Every run since the first; the test suites run three at once. | Implicit in every check |
| `--background` is Mac-only. | Studio's CLI docs. Not needed on Windows. On macOS it's how studio-run hides Studio (see [macOS](#macos)). | — |
| The `roblox-studio` protocol handler (`HKCU\Software\Classes\roblox-studio\shell\open\command`) points at the current `RobloxStudioBeta.exe` and stays correct across updates. | Read the key on a machine with Studio installed. Not yet observed across an update. | `locate::tests` (parsing only) |
| The saved login is reused. The log shows `Authenticated : NO` early on, then `YES`, even when the login works. | Studio logs. | `raw_saved_login_is_reused` |
| `--apiV2 <file>` writes the API dump and exits 0 in about a second. The V2 schema uses lowercase keys (`classes`). | `studio-run api-dump`. | `api_dump_writes_json` in tests/studio.rs |

### Running a script (`--task RunScript --runScriptFile`)

| Finding | How we know | Check |
|---|---|---|
| A run takes about 7–10s end to end on the default baseplate. | Timing every probe. | — |
| **Studio's exit code is 0 whatever the script does.** | `raw_errors.luau` throws, Studio exits 0. | `raw_uncaught_error_still_exits_0` |
| **An uncaught error after the script has yielded is lost**: not in the output, not in the log. Before the first yield it's reported as `RunScript:<line>: <message>`. | `raw_errors.luau` vs `raw_error_after_yield.luau`. | `raw_uncaught_error_before_a_yield_is_reported`, `raw_uncaught_error_after_a_yield_is_lost` |
| Errors in spawned threads are reported as `RunScript:<line>: <message>`, followed by `Stack Begin` / `Script 'RunScript', Line <n>` / `Stack End` info lines. | `raw_errors.luau`. | `raw_spawned_thread_errors_are_reported_as_runscript` |
| Threads the script spawns keep running after its main chunk returns, until Studio quits. | `task.delay` print in `raw_errors.luau` appears after the chunk has returned. | `raw_spawned_threads_outlive_the_chunk` |
| **A UTF-8 byte-order mark makes Studio silently run nothing.** | Same script with and without a BOM. | `raw_bom_makes_studio_run_nothing` |
| The source is echoed into the output first, as one message starting with `> `. | Output file and log. | `raw_source_is_echoed` |
| `--localPlaceFile` loads the place, and changes aren't saved on quit (no save prompt either). | `raw_place.luau` changes `marker.rbxlx`; the file is unchanged afterwards. | `raw_local_place_file_is_not_saved` |
| **`--quitAfterExecution` is unreliable.** Sometimes Studio goes back to its start page instead of exiting, and once it crashed on exit (`0xC0000005`). Recent runs mostly quit cleanly; earlier about 1 in 3 didn't. | Repeated runs. | `raw_quit_after_execution_reliability` (measurement, read with `--nocapture`) |
| The process's stdout is empty for `RunScript`, despite the docs. | Early experiments with redirected stdout. studio-run now gives Studio null handles. | — |
| Plugins installed for the user (including cloud plugins) load and print, some after the script has finished. | Output of every run. | — |

### Output

| Finding | How we know | Check |
|---|---|---|
| **`--outputFile` must end in `.log`.** Otherwise Studio logs `Output file must have a .log extension` and writes nothing. | `raw_output_file_needs_log_extension`. | `raw_output_file_needs_log_extension` |
| **The output file is buffered.** It's created empty at startup, and short output only reaches it when the place closes. A large amount of output (a 5,000-character line) is flushed sooner. | Polling the file every second while `raw_output.luau` pauses. | `raw_output_file_is_written_only_at_the_end` |
| The output file is written when the place closes, even if Studio then fails to quit. | A run that stayed at the start page had a complete output file. | — |
| **Studio's log is written live**: output appears within about a second. | Polling the log while `raw_output.luau` pauses. | `raw_log_is_written_live` |
| **Log lines are cut at about 1,000 characters** (1,022 including the timestamp prefix). The output file keeps the whole line. | `raw_output.luau` prints a 5,000-character line. | `raw_log_lines_are_cut_near_1000_chars`, `raw_output_file_keeps_long_lines` |
| `LogService.MessageOut` delivers messages up to 100,000 characters intact and cuts longer ones to about 100,000. | `message_size.luau`. | `message_out_cuts_near_100k_chars` |

### What a script can use

Scripts run with command-bar permissions.

| Finding | How we know | Check |
|---|---|---|
| Available: `loadstring`, `setfenv`/`getfenv`, `debug.info`, `debug.traceback`, `LogService.MessageOut`, `ScriptContext.Error`, reading `ModuleScript.Source`. | `capabilities.luau`. | `script_capabilities` |
| **Not available: `ProcessService`** (so a script can't set Studio's exit code), `debug.loadmodule`, `FileSystemService`. | `capabilities.luau`. | `script_capabilities` |
| An error in a thread spawned by a `loadstring` chunk reaches `ScriptContext.Error` with `origin` nil and a stack that names the chunk (`name.luau, line N`). | `script_errors.luau`. | `spawned_thread_errors_reach_script_context` |
| The globals table that every script's environment falls back to can't be written. | `module_env.luau`. | `module_environments` |
| `setfenv(0, ...)` before `require` doesn't affect the module: each module gets a fresh environment. | `module_env.luau`. | `module_environments` |
| After the real `require`, a module function's environment can be patched, but its top-level code has already run. | `module_env.luau`. | `module_environments` |
| Compiling `ModuleScript.Source` with `loadstring` and our own environment captures all of a module's output, top level included. | `module_env.luau`. | `module_environments` |

## macOS

Checked with Studio **0.748.0.7481526** on 2026-11-22, by running both suites
(`tests/behavior.rs` and `tests/studio.rs`) with Studio logged in. Every run
was started with `--background`, which shows in each run's log header.

- **Every finding above holds on macOS.** All 19 behaviour checks and all 14
  end-to-end tests passed. That includes exit code 0 whatever the script does,
  errors after a yield being lost, the buffered `--outputFile`, and log lines
  cut at 1,022 characters.
- `--quitAfterExecution` quit cleanly in 3 of 3 runs, as in recent Windows
  runs. The wrapper's kill after `result` stays in place anyway.
- Studio is `/Applications/RobloxStudio.app/Contents/MacOS/RobloxStudio`. Its
  log is `~/Library/Logs/Roblox/<version>_<time>_Studio_<id>_last.log`, with
  the same `Command line:` header as on Windows.
- Without a login, a raw run kept Studio open until `--timeout`, the log
  never showed `Authenticated : YES`, and `raw_saved_login_is_reused` failed.
- **`--background` showed nothing**: watching a `studio-run doctor` run, no
  window appeared and focus wasn't taken. Studio's docs promise only that it
  doesn't bring its window to the front, and unlike the Windows private
  desktop, no check verifies this. Watch a `studio-run doctor` after a Studio
  update.

## Corrections

Findings we recorded earlier that turned out to be wrong or incomplete:

- *"Spawned threads are cut off when the main chunk returns."* They aren't:
  they keep running until Studio quits.
- *"The output file is only written at exit."* It's buffered: short output
  appears at the end, but large output is flushed sooner.
- *"The exit code is 0 even when the script throws."* Still true, and on top
  of that, an error after a yield isn't reported at all.
