# studio-run: findings and design notes

Goal: run Roblox Studio CLI tasks on Windows without showing a GUI or
interrupting the user. [README.md](README.md) covers usage; this file records
what we found out about Studio and why the tool works the way it does.

## Status

Built: `run`, `api-dump` and `doctor`, the wrapper, live output from Studio's
log, and jsonl output. 12 unit tests and 14 end-to-end tests against Studio
pass (Studio 0.748.0.7481526).

Still open:
- Running while the user has Studio open themselves.
- Behaviour when the saved login has expired.
- Studio auto-updating, or the first launch after an update.
- A real jest-roblox project ([examples/jest.luau](examples/jest.luau) is unverified).
- CI that builds and runs the unit tests.

## Findings about Studio's CLI (tested 2026-11-20/21)

Probe scripts are in `experiments/`. Run them with
`studio-run run experiments/<name>.luau --no-capture --output verbose`.

| Finding | Detail |
|---|---|
| **A private desktop hides Studio completely** | `CreateDesktopW`, then `STARTUPINFOW.lpDesktop = "WinSta0\\<name>"`. Studio renders and runs normally there. No window appeared on the user's desktop and focus was never stolen. |
| `--background` is Mac-only | Documented as such. We don't need it. |
| Finding Studio | `HKCU\Software\Classes\roblox-studio\shell\open\command` points at the current `Versions\version-*\RobloxStudioBeta.exe` and stays correct across updates. |
| `--apiV2 <file>` | Exits 0 in about 1s. The V2 schema uses lowercase keys (`classes`). |
| `RunScript ... --quitAfterExecution` | About 7–10s end to end on the default baseplate. |
| `--localPlaceFile` | Works. Changes aren't saved, and there's no save prompt on quit. |
| **Exit code is always 0** | Even when the script throws. |
| **Quitting is unreliable** | In about 1 of 3 runs Studio logged `exception while signaling: Unknown exception`, closed the place and went back to the start page instead of exiting. |
| **A UTF-8 BOM breaks scripts silently** | Studio echoes the source but runs nothing, with no error. |
| Yields | The main chunk can yield. Spawned threads are cut off when it returns. |
| stdout | Empty for `RunScript`, contrary to the docs. |
| **`--outputFile` is only written at exit** | So it can't be used to stream output. |
| **Studio's log file is written live** | `%LOCALAPPDATA%\Roblox\logs\*_Studio_*_last.log`. Output appears within about 1s as `[FLog::CreatorOutput]` / `CreatorWarning` / `CreatorError` lines. The header contains the full command line, which identifies the process. |
| **Log lines are cut at about 1,000 characters** | `MessageOut` and `--outputFile` cut messages at about 100,000 characters. |
| Auth | The saved login is reused. The log shows `Authenticated : NO`, then `YES`, even when it works. |
| Output noise | The source is echoed (one message starting with `"> "`), and the user's cloud plugins print messages, some of them after the script has finished. |
| Concurrency | Several simultaneous runs on separate desktops work (tested with 3). |
| Cleanup | A job object with `KILL_ON_JOB_CLOSE` takes down Studio and all its child processes. |

Available under `RunScript` (command-bar permissions): `loadstring`,
`setfenv`/`getfenv`, `debug.info`, `debug.traceback`, `LogService.MessageOut`,
`ScriptContext.Error`, and reading `ModuleScript.Source`.
Not available: `ProcessService`, `debug.loadmodule`, `FileSystemService`.

## Design decisions

**Every run goes through a generated wrapper.** studio-run builds Studio's
command line itself. There is no way to pass your own `--runScriptFile`. The
user's source is embedded as a `\ddd`-escaped string literal, which works for
any bytes, unlike a long-bracket string. The template is in `src/wrapper.luau`
and is compiled into the binary.

**Output categories.** The script's own output is captured exactly by
compiling it with `loadstring(src, "=file.luau")` (so errors show the right
file and line) and running it with its own `print`/`warn` via `setfenv`.
Everything else reaches us through `MessageOut` with a level, and is reported
as `other`. Errors in spawned threads come from `ScriptContext.Error`, are
matched to the script by chunk name, and the copy of the same error that
`MessageOut` also delivers is dropped.

**Events are short, sentinel-prefixed JSON lines**, split into pieces with a
`more` flag to fit the log's line limit. Each has a `seq` so pieces can be
joined back together and duplicates dropped: every event can arrive twice,
once from the live log and once from the output file read at exit. The
sentinel is built from two pieces in the wrapper source, because Studio
echoes that source into its log.

**`result` ends the run.** It's always the last event, and later output (plugin
noise while Studio shuts down) is dropped. After `result`, Studio gets a few
seconds to quit, then it's killed. This works around the unreliable quitting.

**No runner-specific code.** A test runner is just a script. The script's
return value sets the exit code; this replaces `ProcessService`, which doesn't
exist under the CLI. jest-roblox has its own module loader (`loadstring` +
`setfenv`, since `debug.loadmodule` is unavailable) and its own `print`
mocks, so the wrapper changes only the entry chunk's environment. Jest's output
comes through as `other`, which is shown by default.

**ModuleScripts** (`experiments/module_env.luau`):
- The globals table shared by all scripts can't be changed.
- `setfenv(0, ...)` before the real `require` has no effect: each module gets a fresh environment.
- After the real `require`, the module's env (`getfenv(M.fn)`) can be patched, but its top-level code has already run.
- Only loading `Source` ourselves captures everything. That's `--trace-modules`, and it's opt-in because it isn't the real `require`.
