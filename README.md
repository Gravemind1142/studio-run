# studio-run

Run Luau scripts in Roblox Studio on Windows **without Studio ever appearing on
your screen**, and get the script's output and a meaningful exit code back.

```powershell
studio-run run smoke-test.luau --place game.rbxl
```

Studio runs on a private Windows desktop that is never shown, so it can't
steal focus or interrupt you, and several runs can go at once.

## Requirements

- Windows 10 or 11.
- Roblox Studio, installed and **logged in at least once** (studio-run reuses
  your saved login).
- To build from source: Rust (stable, `x86_64-pc-windows-msvc`) and the Visual
  Studio C++ Build Tools.

## Install

With [Foreman](https://github.com/Roblox/foreman), add it to your project's
`foreman.toml` and run `foreman install`:

```toml
[tools]
studio-run = { github = "Gravemind1142/studio-run", version = "0.1.0" }
```

[Rokit](https://github.com/rojo-rbx/rokit) and
[Aftman](https://github.com/LPGhatguy/aftman) can install the same releases:

```powershell
rokit add Gravemind1142/studio-run
```

Each [GitHub release](https://github.com/Gravemind1142/studio-run/releases)
also has a zip with `studio-run.exe` in it. Or build from source:

```powershell
cargo install --git https://github.com/Gravemind1142/studio-run
```

Then check everything works:

```powershell
studio-run doctor
```

## Running a script

```powershell
studio-run run <script.luau> [options] [-- <extra Studio args>]
```

| Option | Default | |
|---|---|---|
| `--place <file>` | empty baseplate | Open a local `.rbxl`/`.rbxlx` first. It's never saved. |
| `--place-id <id> --universe-id <id>` | | Open a published place first. |
| `--timeout <s>` | 300 | Kill Studio and fail (exit code 124) after this long. |
| `--grace <s>` | 5 | How long Studio may take to quit after the script finishes before it's stopped. |
| `--format text\|jsonl` | text | Text output, or one JSON event per line (see below). |
| `--output script\|normal\|verbose` | normal | Which output to show in text format (see below). |
| `--no-capture` | | Don't replace `print`/`warn` in the script's environment. |
| `--trace-modules` | | Attribute required modules' output to them. See the caveats below. |
| `--visible` | | Show Studio's window on your desktop, for debugging. |
| `--keep-temp` | | Keep the generated wrapper and output file. |
| `--studio <exe>` | auto | Path to `RobloxStudioBeta.exe`. Also `STUDIO_EXE`. |

### Exit codes

| Code | Meaning |
|---|---|
| `0` | The script finished (and returned nothing, `true`, or `0`). |
| `1` | The script threw an uncaught error, didn't compile, or returned `false`. |
| *n* | The script returned the number *n*, or `{ exitCode = n }`. |
| `2` | Bad command-line usage. |
| `124` | Timed out. |
| `125` | studio-run problem: Studio not found, failed to start, or exited without the script reporting a result. |

### Writing scripts

The script runs in Studio's edit mode with command-bar permissions, like
`--task RunScript` normally does.

- **Return a value to set the exit code**: `return true`/`false`, a number,
  or `{ exitCode = n }`. This works for any script, test runners included.
- The script may **yield** (`task.wait`, promises, etc.); the run lasts until
  its main chunk returns.
- Threads the script spawns (`task.spawn`, event connections) are **not**
  waited for. Wait for your own background work before returning. Errors in
  spawned threads are reported (with `async: true`) but don't fail the run.
- Errors and tracebacks point at your file and line, e.g. `test.luau:12`.
- `script` is `nil`.
- A UTF-8 byte-order mark at the start of the file is ignored. Studio itself
  silently refuses to run a script that starts with one.

## Output

Every piece of output is put in one of two groups:

- **`script`**: your script's `print`/`warn` calls, its uncaught error, and
  errors in threads it spawned.
- **`other`**: everything else Studio prints while the script runs. That
  includes modules your script `require`s (they use the real `print`), your
  installed plugins, and Studio itself.

In **text format**, `print` output goes to stdout. Warnings and errors go to
stderr, prefixed with `warning:` / `error:`. studio-run's own messages go to
stderr, prefixed with `studio-run:`. `--output` picks what's shown:

- `script`: only the script's own output.
- `normal`: the script's output, plus `print`/`warn`/`error` from other sources.
  Output from test runners and modules shows up here, along with some plugin
  noise.
- `verbose`: also Studio's info-level messages.

Output is streamed while the script runs.

In **jsonl format**, every event is printed (no filtering), one per line:

```json
{"seq":1,"kind":"start","chunk":"test.luau"}
{"seq":2,"kind":"output","source":"script","level":"print","message":"hello"}
{"seq":5,"kind":"output","source":"other","level":"warn","message":"..."}
{"seq":7,"kind":"output","source":"script","level":"error","async":true,"message":"test.luau:3: boom\ntest.luau, line 3\n"}
{"seq":9,"kind":"result","ok":true,"exitCode":0,"reason":"returned"}
```

| Field | Values |
|---|---|
| `kind` | `start`, `output`, `result`. `result` is always last. |
| `source` | `script` or `other` |
| `level` | `print`, `info`, `warn`, `error` |
| `reason` (result) | `returned`, `error`, `compile` |

If Studio times out or never reports a result, there is no `result` event;
use the exit code.

**Message size**: your script's own output can be any length. Output in
the `other` group is cut off by Studio itself at about 100,000 characters per
message.

## Test runners

A test runner is just a script: run it, and have it return whether the tests
passed. [examples/jest.luau](examples/jest.luau) does this for
[jest-roblox](https://github.com/Roblox/jest-roblox):

```powershell
studio-run run examples/jest.luau --place tests.rbxl
```

Jest's reporter output appears in the `other` group (Jest's modules use the
real `print`), so leave `--output` at `normal`, the default. Jest's usual
entry script ends with `ProcessService:ExitAsync`. That service doesn't exist
under Studio's CLI, so `return` the result instead.

Don't use `--trace-modules` with test runners. They have their own module
loader.

## `--trace-modules`

Normally your script's `require` is the real one, so a module's output is in
the `other` group. With `--trace-modules`, `require(moduleScript)` instead
loads the module's `Source` through studio-run. Each module gets the same
captured `print`/`warn` and its own `script`, so its output and errors are
in the `script` group, named after it (e.g. `ReplicatedStorage.Greeter:8`).

This is **not** the real `require`, so behaviour differs in edge cases:

- Modules are cached per run, but not shared with code that uses the real
  `require`.
- `require(assetId)` and non-ModuleScript arguments fall back to the real
  `require` (untraced).
- Functions running in a custom environment lose some Luau optimizations.

## Other commands

```powershell
studio-run api-dump api.json [--kind v2|full|luau]
```

This writes Roblox's API dump in about a second, with no window.

```powershell
studio-run doctor
```

This finds Studio and runs a test script in it to check your setup.

## How it works

1. A Luau **wrapper** is generated around your script (Studio's Luau can't
   read local files, so your source is embedded in it as an escaped string). It
   compiles your code with `loadstring` under your file's name, runs it in
   `xpcall` with its own `print`/`warn`, and listens to `LogService.MessageOut`
   and `ScriptContext.Error` for everything else. It reports what happens as
   short `@@studio-run@@{json}` lines.
2. Studio is started with `--task RunScript --quitAfterExecution`:
   - on a **private desktop** (`CreateDesktopW`, never switched to), so its
     windows exist but are never shown;
   - inside a **job object**, so Studio and its child processes are killed if
     the run times out, or if studio-run itself exits or is interrupted.
3. studio-run follows **Studio's own log file** in `%LOCALAPPDATA%\Roblox\logs`,
   which is written live, unlike `--outputFile`. It reads the events from
   there and joins messages that were split to fit Studio's ~1,000-character
   log-line limit. The `--outputFile` is read as a fallback at the end.
4. When the `result` event arrives, Studio gets `--grace` seconds to quit by
   itself, then it's stopped. `--quitAfterExecution` occasionally leaves Studio
   at its start page instead of quitting.

## Known limitations

- Windows only (macOS has Studio's own `--background` flag instead).
- Plugin output shows up in the `other` group. There's no way to tell plugin
  output apart from your modules' output.
- Not tested: running while you have Studio open yourself, or after your saved
  login expires. A run that can't log in is likely to hang until `--timeout`.
  studio-run then points out the login problem and the Studio log.
- Relies on Studio's log file format to stream output live. If that changes,
  output still arrives, but only once Studio exits.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute, and
[CHANGELOG.md](CHANGELOG.md) for what changed in each release.

```powershell
cargo test                                # unit tests
cargo test --test studio -- --ignored     # end-to-end tests against real Studio (a few minutes)
cargo test --test behavior -- --ignored   # check Studio still behaves as documented (~2 min)
```

The end-to-end tests (`tests/studio.rs`) need Studio installed and logged in.
They run up to three hidden Studio instances at once. CI can't run them (its
runners have no logged-in Studio), so run them locally before opening a PR
that touches the wrapper, the sandbox or the log reader.

CI (`.github/workflows/ci.yml`) runs on every pull request and push to `main`:
`cargo fmt --check`, `cargo clippy -D warnings`, the unit tests and a release
build.

### Releasing

1. Bump `version` in `Cargo.toml` and commit (`Cargo.lock` updates with it).
2. Tag and push: `git tag v0.2.0` then `git push origin v0.2.0`.

`.github/workflows/release.yml` checks that the tag matches `Cargo.toml`,
builds, and publishes a GitHub release with
`studio-run-<version>-windows-x86_64.zip` and its SHA-256. Foreman and Rokit
find that asset by name, so keep the naming. A tag with a suffix, like
`v0.2.0-rc.1`, becomes a pre-release.

### How Studio behaves

Studio's CLI is mostly undocumented. [docs/studio-behavior.md](docs/studio-behavior.md)
records what we found out about it and how, and [docs/design.md](docs/design.md)
explains how studio-run works around it. `tests/behavior.rs` re-checks each
finding against the installed Studio; run it after a Studio update.
