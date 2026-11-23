# studio-run

Run Luau scripts in Roblox Studio on Windows or macOS **without Studio taking
over your screen**, and get the script's output and a meaningful exit code back.

```powershell
studio-run run smoke-test.luau --place game.rbxl
```

On Windows, Studio runs on a private desktop that is never shown, so it can't
steal focus or interrupt you, and several runs can go at once. On macOS, Studio
is started with its own `--background` flag, which keeps it out of the
foreground. The commands below are the same in either shell.

The same script can also run in the cloud, as an
[Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)
task, with the same output format and exit codes. This works on Windows, Linux
and macOS, with no Studio needed, so it suits CI:

```powershell
studio-run run smoke-test.luau --place game.rbxl --cloud
```

See [Running in the cloud](#running-in-the-cloud---cloud).

## Requirements

For local runs:

- Windows 10 or 11, or macOS.
- Roblox Studio, installed and **logged in at least once** (studio-run reuses
  your saved login).

For `--cloud` runs: Windows, macOS or Linux, an experience to run in and an
Open Cloud API key (see [below](#running-in-the-cloud---cloud)).

To build from source: Rust (stable). On Windows, also the Visual Studio C++
Build Tools.

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
also has a zip per platform (Windows x86_64, macOS aarch64 and x86_64, Linux
x86_64) with the executable in it (`studio-run.exe` on Windows, `studio-run`
elsewhere), and a `SHA256SUMS` file. The Linux build runs `--cloud` only. Or
build from source:

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
| `--arg <key>=<value>` | | Pass a string to the script as `ctx.args[key]`. Repeatable. See [Script context](#script-context). |
| `--no-capture` | | Don't replace `print`/`warn` in the script's environment. |
| `--trace-modules` | | Attribute required modules' output to them. See the caveats below. |
| `--visible` | | Show Studio's window on your desktop, for debugging. |
| `--keep-temp` | | Keep the generated wrapper and output file. |
| `--studio <exe>` | auto | Path to the Studio executable, or on macOS the `.app` bundle. Also `STUDIO_EXE`. |
| `--cloud` | | Run as an Open Cloud Luau Execution task instead. See [below](#running-in-the-cloud---cloud). |

### Exit codes

| Code | Meaning |
|---|---|
| `0` | The script finished (and returned nothing, `true`, or `0`). |
| `1` | The script threw an uncaught error, didn't compile, or returned `false`. |
| *n* | The script returned the number *n*, or `{ exitCode = n }`. |
| `2` | Bad command-line usage. |
| `124` | Timed out. |
| `125` | studio-run problem: Studio not found, failed to start, or exited without the script reporting a result. With `--cloud`: the API refused a request (key, permissions, IDs), the place upload failed, or the task failed without the script reporting a result. |

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

### Script context

The script's first vararg is a table describing the run, the same shape
locally and under `--cloud`:

```luau
local ctx = ...
ctx.host     -- "local" or "cloud"
ctx.version  -- studio-run's version, e.g. "0.1.0"
ctx.args     -- { [string]: string }, from --arg key=value
ctx.place    -- { universeId: number?, placeId: number?, version: number? }
```

`--arg` splits at the first `=`, so values may contain `=`; giving the same
key twice is a usage error. Locally, `ctx.place` has the IDs only with
`--place-id`, and never a version. Under `--cloud` it has the universe, place
and the version the task ran on.

Use `ctx.host` to skip what only one host can do. For example, server-only
tests can only pass under `--cloud`, because Studio runs scripts in edit mode
(see [Differences from local runs](#differences-from-local-runs)):

```luau
local ctx = ...
if ctx.host == "local" then
	print("skipping server-only tests")
end
```

Only the script itself gets the context; required modules don't.

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
| `kind` | `start`, `output`, `result`, `error`. `result` is always last when there is one. |
| `host` (start) | `local` or `cloud` |
| `universeId`, `placeId` (start) | The place the script ran in, when it's a published one. |
| `placeVersion`, `taskId` (start) | `--cloud`: the place version and the task. |
| `source` | `script` or `other` |
| `level` | `print`, `info`, `warn`, `error` |
| `reason` (result) | `returned`, `error`, `compile` |
| `code` (error) | `--cloud` failures: `auth`, `permission`, `not_found`, `invalid_request`, `rate_limited`, `server`, `http`, `network`, `upload_failed`, `task_failed` |

If the run times out or never reports a result, there is no `result` event;
use the exit code. An `error` event explains a `125` exit under `--cloud`.

**Message size**: your script's own output can be any length. Output in
the `other` group is cut off by Studio itself at about 100,000 characters per
message.

## Running in the cloud (`--cloud`)

With `--cloud` (or `STUDIO_RUN_HOST=cloud`), the script runs as an
[Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)
task in a place you own, instead of in local Studio. Output, `--format`,
`--output` and exit codes are the same. It works on Windows, Linux and macOS.

### Setup

1. **An experience for the runs.** Tasks use the experience's **real**
   DataStores and other cloud data, so use one that exists only for this, not
   your game. An empty Baseplate published from Studio is enough.
2. **An API key** from the
   [Creator Dashboard](https://create.roblox.com/dashboard/credentials),
   restricted to that experience, with:
   - **Luau execution sessions**: read and write
     (`universe.place.luau-execution-session:read`/`:write`).
   - **Places**: write (`universe-places:write`), if you use `--place`.
   - **Accepted IP addresses**: your IP. CI runners such as GitHub's have
     changing IPs and need `0.0.0.0/0`.
3. Put the key in the `ROBLOX_API_KEY` environment variable (or another,
   named with `--api-key-env`). The key is never accepted as an argument, so it
   can't end up in shell history or CI logs.
4. Name the place, either with `--universe-id` and `--place-id` or in a
   `studio-run.toml` in your project (found in the current directory or a
   parent):

   ```toml
   [cloud]
   universe-id = 1234567890
   place-id = 9876543210
   api-key-env = "ROBLOX_API_KEY"   # optional
   upload = true                    # optional: this place is disposable; let --place upload to it
   ```

5. Check it: `studio-run doctor --cloud` runs a trivial task.

### Options

| Option | Default | |
|---|---|---|
| `--place <file>` | | Upload the file as a new **Saved** (unpublished) version of the place, then run on it. Needs `--allow-upload` or `upload = true`, because the upload becomes the place's latest version. |
| `--place-version <n>` | latest | Run on an existing version, without uploading. |
| no place given | | Run on the place's latest version (Saved versions included). |
| `--timeout <s>` | 300 | The task's timeout: at most 300, Roblox's limit. |
| `--queue-timeout <s>` | 120 | Extra time allowed for uploading and for the task to start. The run gives up after `--queue-timeout` + `--timeout`. |
| `--poll-interval <s>` | 2 | Seconds between task status checks. |
| `--api-key-env <name>` | `ROBLOX_API_KEY` | Environment variable holding the key. |
| `--config <file>` | `studio-run.toml` | Config file to use. |
| `--keep-temp` | | Keep the generated task script and the final task (without its download URL). |

Flags win over `studio-run.toml`. `--grace` and `--studio` are ignored with
`--cloud`; `--visible` and extra Studio arguments are usage errors (exit 2).

### Differences from local runs

The task is a headless server, not Studio's edit mode. What we found is in
[docs/cloud-behavior.md](docs/cloud-behavior.md); in short:

- **Output arrives when the task finishes**, not while it runs: Roblox only
  makes it available then. With `--output verbose`, text format shows the
  upload and task progress as `studio-run:` notes.
- No plugins, no Plugin capability, no `loadstring`. `RunService:IsServer()`
  is true; `IsRunning()` and `IsEdit()` are false. The place's own scripts
  don't run and physics doesn't step.
- HttpService follows the experience's **Allow HTTP Requests** setting.
- `script` is `nil`, as locally. Errors and tracebacks point at your file and
  line, as locally.
- Your script is compiled as the body of a function, so a compile error at
  the very end of the file (an unfinished statement) is reported one line
  past it, at `end`, under `--cloud`. With `--trace-modules`, the same goes
  for required modules.
- The task script, with your source and `--arg` values embedded, is limited
  to 4 MB.
- A `.rbxlx` file has trailing whitespace trimmed before upload: Roblox rejects
  XML with a newline after `</roblox>`.
- **Tasks can't be cancelled.** If studio-run gives up waiting (or you press
  Ctrl+C), the task keeps running until it finishes or reaches its timeout.
- Roblox allows 5 task creations per minute per account (across all your keys)
  and 10 unfinished tasks per place. studio-run retries rate-limited requests
  until `--queue-timeout` runs out.

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
- Under `--cloud`, each module is compiled as the body of a function (tasks
  have no `loadstring`), so a compile error at the very end of a module is
  reported one line late, at `end`.

## Other commands

```powershell
studio-run api-dump api.json [--kind v2|full|luau]
```

This writes Roblox's API dump in about a second, with no window.

```powershell
studio-run doctor
```

This finds Studio and runs a test script in it to check your setup.
`studio-run doctor --cloud` instead checks your cloud settings and key and runs
a trivial task.

## How it works

1. A Luau **wrapper** is generated around your script (Studio's Luau can't
   read local files, so your source is embedded in it as an escaped string). It
   compiles your code with `loadstring` under your file's name, runs it in
   `xpcall` with its own `print`/`warn`, and listens to `LogService.MessageOut`
   and `ScriptContext.Error` for everything else. It reports what happens as
   short `@@studio-run@@{json}` lines.
2. Studio is started with `--task RunScript --quitAfterExecution`:
   - on Windows, on a **private desktop** (`CreateDesktopW`, never switched
     to), so its windows exist but are never shown, and inside a **job
     object**, so Studio and its child processes are killed if the run times
     out, or if studio-run itself exits or is interrupted;
   - on macOS, with **`--background`**, so Studio stays out of the foreground,
     and in its own **process group**, killed the same way on timeout or exit.
3. studio-run follows **Studio's own log file** (`%LOCALAPPDATA%\Roblox\logs`
   on Windows, `~/Library/Logs/Roblox` on macOS), which is written live, unlike
   `--outputFile`. It reads the events from there and joins messages that were
   split to fit Studio's ~1,000-character log-line limit. The `--outputFile` is
   read as a fallback at the end.
4. When the `result` event arrives, Studio gets `--grace` seconds to quit by
   itself, then it's stopped. `--quitAfterExecution` occasionally leaves Studio
   at its start page instead of quitting.

## Known limitations

- On macOS, hiding Studio means its `--background` flag. That keeps the window
  out of the foreground today. It is not a private desktop, and a later Studio
  build could show a window. Re-run the behaviour checks after a Studio update.
- Local runs need Windows or macOS. On Linux, only `--cloud` works.
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
cargo test                                # unit tests, and --cloud against a mock API
cargo test --test studio -- --ignored     # end-to-end tests against real Studio (a few minutes)
cargo test --test behavior -- --ignored   # check Studio still behaves as documented (~2 min)
cargo test --test cloud -- --ignored --test-threads=1            # end-to-end --cloud tests (~3 min)
cargo test --test cloud_behavior -- --ignored --test-threads=1   # check Open Cloud still behaves as documented
```

The real-API cloud tests need a test experience and API key of your own, in a
gitignored `.env`: see [docs/cloud-testing.md](docs/cloud-testing.md).
`.github/workflows/cloud.yml` runs them weekly and on manual runs, with the
maintainers' secrets, never on pull requests. On pull requests,
`tests/cloud_mock.rs` checks `--cloud` against a local mock of the API instead.

The end-to-end tests (`tests/studio.rs`) need Studio installed and logged in.
They run up to three hidden Studio instances at once. CI can't run them (its
runners have no logged-in Studio), so run them locally before opening a PR
that touches the wrapper, the sandbox or the log reader.

CI (`.github/workflows/ci.yml`) runs on every pull request and push to `main`,
on Windows, macOS and Linux: `cargo fmt --check`, `cargo clippy -D warnings`,
the unit tests and a release build.

### Releasing

1. Bump `version` in `Cargo.toml` and commit (`Cargo.lock` updates with it).
2. Tag and push: `git tag v0.2.0` then `git push origin v0.2.0`.

`.github/workflows/release.yml` checks that the tag matches `Cargo.toml`,
builds, and publishes a GitHub release with these zips and one `SHA256SUMS`
file:

- `studio-run-<version>-windows-x86_64.zip`
- `studio-run-<version>-macos-aarch64.zip`
- `studio-run-<version>-macos-x86_64.zip`
- `studio-run-<version>-linux-x86_64.zip` (`--cloud` only)

Foreman, Rokit and Aftman find the asset for the current OS by name, so keep
the naming. Keep the checksums in the single `SHA256SUMS` file: a
`<zip>.sha256` file also matches Foreman's name search, and Foreman would try
to install it if it were listed first. A tag with a suffix, like
`v0.2.0-rc.1`, becomes a pre-release.

### How Studio behaves

Studio's CLI is mostly undocumented. [docs/studio-behavior.md](docs/studio-behavior.md)
records what we found out about it and how, and [docs/design.md](docs/design.md)
explains how studio-run works around it. `tests/behavior.rs` re-checks each
finding against the installed Studio; run it after a Studio update.

## License

[MIT](LICENSE)
