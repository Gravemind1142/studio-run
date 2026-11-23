# studio-run: cloud host (`--cloud`)

Status: spec, nothing built. Targets studio-run v0.2. Written against the v0.1.0 README.

## 1. Goal

Run the same script, with the same arguments, either in a hidden local Studio (today) or
in an Open Cloud Luau Execution task, and get the same output format and exit codes back:

```powershell
studio-run run tests/run.luau --place build/test.rbxl            # local Studio, Windows
studio-run run tests/run.luau --place build/test.rbxl --cloud    # Luau Execution task
```

This replaces rocale-cli for the template (§5.0 of the architecture doc): one tool, one
entry script, one output format, and CI that parses one thing.

**Flag name:** `--cloud`, with `--rocale` as an alias. The host is Open Cloud Luau
Execution; "rocale" is one client for it, so it shouldn't be the main name.

### Non-goals

- Making the two hosts identical. A Luau Execution task is a headless server, not Studio
  edit mode; the differences are listed and exposed to scripts (§6), not hidden.
- Place management beyond what a run needs (copying, publishing): that's place-promote.
- Wrapping rocale-cli. See §3.

## 2. What changes, in summary

| Area | Change |
|---|---|
| Architecture | A `Host` trait with `LocalStudio` and `Cloud` implementations; the wrapper, event parsing, output and exit codes are shared |
| CLI | `--cloud` plus cloud options (§4.2); every existing option gets defined cloud behaviour (§4.1) |
| Config | Optional `studio-run.toml` so `--cloud` needs no IDs on the command line (§5) |
| Wrapper | A cloud mode that compiles the user's source as a ModuleScript (tasks have no `loadstring`) and returns events in the task's binary output instead of Studio's log file (§7) |
| Script context | A `studio-run` context table telling the script its host, args and inputs, on both hosts (§6) |
| Inputs/outputs | `--arg`, `--input` and `--binary-output` on both hosts (§4.3) |
| Output | Same text and jsonl formats; `start` gains host details; a new `error` event (§8) |
| Exit codes | Unchanged meanings; cloud failures map onto them (§9) |
| Platforms | Cloud mode works on Linux and macOS; new release assets (§11) |
| Commands | `doctor --cloud` (§10) |
| Tests | A parity suite run on both hosts (§12) |

## 3. Native client, not a rocale-cli wrapper

Implement the Open Cloud calls in Rust inside studio-run rather than shelling out to
rocale-cli:

- rocale-cli is a Lute binary with its own output, and its exit codes aren't documented.
  Mapping them onto studio-run's would be guesswork.
- studio-run needs the task logs to produce its event stream, and the exact task request
  (binary input, binary output, timeout). Owning the client gives both.
- It avoids a second tool in `foreman.toml` and a second release cadence.

The client is small: upload a place version, create a binary input, create a task, poll
it, read its logs, download its binary output.

## 4. Command line

### 4.1 Existing options under `--cloud`

| Option | Cloud behaviour |
|---|---|
| `<script.luau>` | Same. Embedded in the task script (§7). |
| `--place <file>` | Uploaded to the configured place as a **Saved** (unpublished) version, then the task runs on that exact version. Needs `--allow-upload` or `upload = true` in config (§5), because the new version becomes the place's latest, which is what Studio and Team Create open next, and what tasks created without a version run on. An `.rbxlx` has trailing whitespace trimmed before upload, because the API rejects XML with a newline after `</roblox>`. |
| `--place-id --universe-id` | The place to run on (and to upload to with `--place`). Can come from config. |
| no place given | Locally: empty baseplate. Cloud: runs on the configured place's latest version, or `--place-version`. "Latest" is resolved to a version number once, before the task is created, and reported in the `start` event, so a concurrent upload can't change it mid-run. |
| `--timeout <s>` | Sent as the task timeout: how long the script may run. Exit 124 on expiry. Must be ≤ the API's task limit (5 minutes, which is also the API's default); larger is a usage error (exit 2). Upload and queue time don't count against it (see `--queue-timeout`). |
| `--grace <s>` | Ignored (no process to stop). Accepted so the same args work on both hosts. |
| `--format text\|jsonl` | Same formats (§8). |
| `--output script\|normal\|verbose` | Same filtering. In the cloud there are no plugins, so `normal` is quieter. |
| `--no-capture` | Same. |
| `--trace-modules` | Implemented with ModuleScripts instead of `loadstring` (§7.2), if tasks can read place modules' `Source` (**⚠ verify**); otherwise a usage error under `--cloud`. |
| `--visible` | Usage error under `--cloud`. |
| `--keep-temp` | Keeps the generated task script and the raw API responses, with signed URLs and auth headers redacted. |
| `--studio <exe>` | Ignored under `--cloud`. |
| `-- <extra Studio args>` | Usage error under `--cloud`. |

"Ignored" options print one `studio-run:` note to stderr, so a shared argument list
doesn't fail but nobody is surprised.

### 4.2 New cloud options

| Option | Default | |
|---|---|---|
| `--cloud` / `--rocale` | off | Run as a Luau Execution task. Also `STUDIO_RUN_HOST=cloud`. |
| `--place-version <n>` | latest | Run on an existing version without uploading. Conflicts with `--place`. |
| `--allow-upload` | off | Permit `--place` to create a Saved version on the cloud place. |
| `--api-key-env <NAME>` | `ROBLOX_API_KEY` | Env var holding the Open Cloud key. The key is never accepted as an argument, so it can't end up in shell history or CI logs. |
| `--poll-interval <s>` | 2 | Seconds between task status polls. |
| `--queue-timeout <s>` | 120 | Extra time allowed for uploading and for the task to start. The run's wall-clock deadline is `--queue-timeout` + `--timeout`; passing it is exit 124. |
| `--config <file>` | `studio-run.toml` if present | §5. |

### 4.3 New options on both hosts

These exist so scripts get the same inputs everywhere:

| Option | Local | Cloud |
|---|---|---|
| `--arg key=value` (repeatable) | Embedded in the wrapper | Embedded in the task script, which is limited to 4 MB in all (user source included) |
| `--input name=<file>` (repeatable) | Embedded in the wrapper as an escaped string literal (`wrapper::lua_string`, as the user's source already is) and turned into a buffer with `buffer.fromstring`, which takes milliseconds. Not base64: Luau has no built-in decoder, and a Luau one took 3.2 s for 16 MB (§14). Escaping makes random binary about 2.9× larger in the wrapper, but Studio accepted a 256 MB literal with no slowdown, so the cap is set to the cloud's 100 MiB for parity | A task takes one binary input, so all inputs are packed into one with a small index. Total ≤ 100 MiB. The wrapper unpacks it from the task's first argument (`({...})[1].BinaryInput`). |
| `--binary-output <file>` | The script's output buffer is sent back as chunked events and reassembled. Each event is one log line of ≤ 600 bytes, base64-encoded. Up to 16 MB of lines adds about 6 s to a run, but 64 MB took 155 s (§14), so this is capped at 8 MiB; larger output fails the run with exit 125 | Carried in the task's binary output (≤ 256 MiB) alongside the event stream (§7.3), and downloaded from `binaryOutputUri`, which is valid for 15 minutes after the task completes |

The script reads them through the context (§6) and sets binary output with
`ctx.setBinaryOutput(buf)`.

## 5. Config file

`studio-run.toml`, looked up from the current directory upward. Optional; flags win.

```toml
[cloud]
universe-id = 111
place-id = 444
api-key-env = "TEST_KEY"
upload = true          # this place is a scratch test place; --place may overwrite it
```

`upload = true` is the explicit "this place is disposable" marker. Without it, `--place`
under `--cloud` needs `--allow-upload` on each call. studio-run never uploads to a place
without one of the two.

### 5.1 API key

The README documents the key studio-run needs:

- **Scopes:** `universe.place.luau-execution-session:write` (creating tasks and binary
  inputs; it also covers reading tasks and logs, which on their own need `:read`), and
  `universe-places:write` if `--place` uploads are used. `doctor --cloud` names any missing
  scope.
- **Rate limits are per key owner**, across all their keys: 5 task creations and 5 binary
  input creations per minute, 200 task reads and 45 log reads per minute. Several CI jobs
  under one owner share these.
- **IP restrictions:** GitHub-hosted runners have changing IPs, so a CI key needs
  `0.0.0.0/0` or a self-hosted runner with a fixed IP.
- **Scope it to the test universe only**, since tasks there touch real DataStores (§6).

## 6. Script context (both hosts)

The wrapper passes the script a context as its first vararg, and also as a global
`STUDIO_RUN`, so existing scripts that ignore it keep working:

```luau
local ctx = ... or STUDIO_RUN
ctx.host            -- "local" | "cloud"
ctx.version         -- studio-run version
ctx.args            -- { [string]: string } from --arg
ctx.inputs          -- { [string]: buffer } from --input
ctx.setBinaryOutput -- (buffer) -> ()
ctx.place           -- { universeId: number?, placeId: number?, version: number? }
```

The **known differences** between hosts, which scripts may need to branch on (the
template's `tests/run.luau` skips `tests/server/` when `ctx.host == "local"`):

| | Local (Studio edit mode) | Cloud (Luau Execution) |
|---|---|---|
| Security level | Command bar | No Plugin capability ("lacking capability Plugin"); `CoreGui` and `StudioService` unreachable |
| `RunService` | edit, **⚠ verify** | `IsServer` only: not client, Studio, edit, running or run mode |
| Physics | Not running | Not running (documented) |
| Place scripts | Don't run | Don't run (documented); module scripts can be required |
| DataModel changes | Discarded | Discarded; local to the task |
| Plugin APIs (`plugin`, `ChangeHistoryService`) | Available | Not available |
| `loadstring` | Available | **Not available**; the wrapper compiles via ModuleScripts instead (§7.1) |
| DataStores, MessagingService, MemoryStore | Off for the baseplate and local files. For a published place with Studio's API access setting on, real, for its universe. | Always **real**, for the place's universe. A test place must be in its own universe. |
| HttpService | Studio setting | The experience's **Allow HTTP Requests** setting (off by default) |
| Other output | Plugins, Studio, modules | Modules and engine only |

These are recorded in [docs/cloud-behavior.md](docs/cloud-behavior.md), like
`docs/studio-behavior.md`, and re-checked by `tests/cloud_behavior.rs` (§12).

## 7. Wrapper in cloud mode

### 7.1 Assembling the task script

Locally, the wrapper compiles the user's source with `loadstring` under the file's name.
Tasks have no `loadstring`, so the cloud wrapper compiles it as a **ModuleScript**
instead. All of this was checked in real tasks ([docs/cloud-behavior.md](docs/cloud-behavior.md)):

```luau
-- in the task script (the wrapper), with SOURCE embedded as a string literal
local m = Instance.new("ModuleScript")
m.Name = "tests/run.luau"                                -- the chunk name locally
m.Source = "return function(...) " .. SOURCE .. "\nend"  -- source starts on line 1
local ok, fn = pcall(require, m)
-- then, as locally: setfenv(fn, env) to capture print, xpcall(fn, handler, ctx)
```

- **Errors and tracebacks name the user's file and line** (`tests/run.luau:3: …`), with no
  remapping. The only other traceback frames are the wrapper's own, `TaskScript:<n>`, which
  are dropped as the local wrapper drops its `RunScript:` frames.
- **The user's code is its own chunk**, so none of the wrapper's locals are in scope for it.
- **Compile errors stay inside the task.** `require` raises only "Requested module
  experienced an error while loading", but the real message (`tests/run.luau:2: Expected
  …`) arrives through `LogService.MessageOut`. The wrapper takes it from there and reports
  it with `reason: "compile"` and exit 1, exactly as locally. The task itself completes.
- **Async errors** in threads the user's code spawns reach `ScriptContext.Error` with the
  file name in the stack, which the local wrapper's attribution already matches.
- **One difference from local:** the source is a function body, so top-level `export type`
  is a compile error under `--cloud`. The README says so. (Requiring the source verbatim
  would allow it, but then the script must return exactly one value and can't get `...`.)

The task script itself (the wrapper, generated by studio-run) should never fail to
compile. If it does, the task fails with `SCRIPT_ERROR` and a `TaskScript:<line>` message,
which studio-run reports as exit 125.

### 7.2 `--trace-modules`

Locally it loads module sources with `loadstring`. In the cloud it uses the same trick as
§7.1: for each required ModuleScript, a new ModuleScript named after the original's full
name, whose source wraps the original's in `return function() … end`; the wrapper requires
it, sets its environment and calls it once, caching the result as `require` would. Reading
`Source` works on modules the task creates; whether it works on modules that are part of
the place is **⚠ verify** (the test experience had none). If it doesn't,
`--trace-modules --cloud` is a usage error.

### 7.3 Capturing output

The wrapper listens to `LogService.MessageOut` and `ScriptContext.Error` and builds the
same events as locally, including "other" output. Only the transport differs.

Locally, events reach studio-run as `@@studio-run@@{json}` lines in Studio's log file. In
the cloud they go back through the task's **binary output**, not its logs, because the
task logs:
- are only documented as readable after the task completes, so they can't stream anyway;
- keep only the last 450 KB per task and discard older output, which a test run that
  prints a lot would exceed, losing the start of the run (wrapping each line as JSON makes
  that worse).

The binary output allows 256 MiB. The wrapper always enables it and returns a
`LuauExecutionTaskOutput` holding one buffer: the event stream as JSON lines, followed by
the script's own binary output (§4.3), with a short header giving the length of each. Its
`ReturnValues` hold just the result (§7.4).

The task logs are a fallback, read when the task ends without returning: a timeout
(`DEADLINE_EXCEEDED`, whose logs keep the output from before it), or a failure in the
wrapper itself. The wrapper also prints each event as a `@@studio-run@@{json}` line for
this case, and studio-run parses whatever is left of them, with a note when the logs were
truncated.
Lines without the prefix are dropped, as every one of them is also wrapped; with
`--output verbose` they are shown as `raw` output, which helps when the wrapper itself
fails before it can emit anything.

### 7.4 Result

The wrapper reports the result in the task's `ReturnValues` (JSON, well under the 4 MB
limit on return values) and as a `result` event. studio-run uses the return value, then
the event from the binary output, then one from the logs.

## 8. Output

Text and jsonl formats are unchanged. Additions:

```json
{"seq":1,"kind":"start","chunk":"tests/run.luau","host":"cloud","universeId":111,"placeId":444,"placeVersion":58,"taskId":"..."}
{"seq":2,"kind":"status","state":"uploading"}
{"seq":3,"kind":"status","state":"queued"}
{"seq":4,"kind":"status","state":"running"}
{"seq":40,"kind":"result","ok":true,"exitCode":0,"reason":"returned"}
{"seq":41,"kind":"error","code":"rate_limited","message":"...","retryAfter":12}
```

- `start` gains `host` on both hosts (`"local"` locally), plus the IDs and task ID in the
  cloud.
- `status` events report cloud progress (`uploading`, `queued`, `running`). Text format
  shows them only with `--output verbose`.
- `error` reports host failures with a stable `code` (`auth`, `permission`, `not_found`,
  `rate_limited`, `quota`, `upload_failed`, `task_failed`, `network`). It precedes a 125
  exit. Local mode uses it too (`studio_not_found`, `studio_start_failed`).
- **No streaming:** in the cloud, the script's output arrives in one batch after the task
  finishes (§7.3); only `status` events appear while it runs. The README says so.

## 9. Exit codes

No new codes; cloud cases map onto the existing ones.

| Code | Cloud case |
|---|---|
| `0`, `1`, *n* | From the script's result, exactly as locally |
| `2` | Bad usage, including options that make no sense in the cloud (§4.1) and a timeout above the API limit |
| `124` | The task timed out, or the run passed its wall-clock deadline (`--queue-timeout` + `--timeout`) |
| `125` | Upload failed, auth or permission denied, rate limit not cleared after retries, task failed without a script result, network error. Details in the `error` event. |

**Retries:** `429` and `5xx` on task creation and polling are retried with backoff
(respecting `retry-after`, and `x-ratelimit-reset` when present) until the wall-clock
deadline. A `429` on task creation is always safe to retry, because no task was created.
An upload is retried only on errors that prove no version was created. A task is never
created twice for one run.

**Cancellation:** the API documents a `CANCELLED` state but no call to cancel a task, and
`PATCH`, `POST …:cancel` and `DELETE` on a running task all return 404. When
the deadline passes, or on Ctrl+C / SIGTERM, studio-run stops waiting and exits; the task
keeps running until it finishes or reaches its own timeout (at most 5 minutes), and holds
one of the place's task slots until then. studio-run prints the task ID so it can be found.
If a cancel call appears, studio-run uses it.

**Concurrent runs on one place:** each run pins the version it uploaded or resolved
(§4.1), so parallel CI jobs on one test place don't interfere with each other's code. A
place allows at most ten incomplete tasks; an eleventh is refused with `429` and retried
as above, so it counts against `--queue-timeout`. Task creation is also limited to 5 per
minute per key owner (§5.1), which in practice is the tighter limit for parallel CI.

## 10. `doctor --cloud`

Checks, in order, printing pass/fail for each:
1. A config or flags name a universe and place.
2. The API key env var is set.
3. The key can read the place, create a task, and (if uploads are allowed) upload. Uses a
   trivial task (`return true`) on the latest version. A failure names the missing scope
   or the IP restriction (§5.1) where the API says which.
4. Reports the security level, `loadstring` availability and `RunService` state seen
   inside the task, as a quick check against `docs/cloud-behavior.md`.

## 11. Platforms and releases

- The local host stays Windows-only; under `#[cfg(windows)]`.
- `--cloud` works on Windows, Linux and macOS. Without `--cloud`, non-Windows builds exit
  2 with "local Studio runs need Windows; use --cloud".
- New release assets alongside the Windows zip: `studio-run-<version>-linux-x86_64.zip`,
  `studio-run-<version>-macos-x86_64.zip`, `studio-run-<version>-macos-aarch64.zip`.
  Checked against the Foreman, Rokit and Aftman sources: all three pick the right zip on
  each platform, and prefer `aarch64` on Apple Silicon. Two conditions:
  - **Always ship both macOS zips.** Foreman's Intel macOS list ends with a bare `macos`,
    so if only the `aarch64` zip existed, Intel Macs would install it and fail to run it.
  - **Checksums go in one `SHA256SUMS` file, not a `.sha256` per zip.** Foreman matches
    asset names by substring and takes the first match in the release's asset list, so
    `…-linux-x86_64.zip.sha256` also matches `linux-x86_64`. If GitHub lists it before the
    zip, Foreman tries to open it as a zip and fails. v0.1.0 has this layout and works only
    because its zip is listed first; the release workflow changes now, before more assets
    are added.
- Each zip holds just the executable at its root (`studio-run.exe`, or `studio-run` with no
  extension on Linux and macOS), with the executable bit set. Foreman installs the zip's
  first entry whatever its name, and Aftman looks for exactly `studio-run` at the root.
- CI builds all targets; unit tests run everywhere.

## 12. Testing

- **Unit:** task-script assembly, log parsing, result mapping, retry logic (against a
  mocked HTTP server), config precedence, the two deadlines, giving up on deadline and
  Ctrl+C, dropping raw log lines, and redaction in `--keep-temp`.
- **Setup** for every cloud test: a dedicated test experience and key, given in
  `STUDIO_RUN_TEST_UNIVERSE_ID`, `STUDIO_RUN_TEST_PLACE_ID` and `STUDIO_RUN_TEST_API_KEY`,
  from the environment or a gitignored `.env` that the tests load themselves
  ([docs/cloud-testing.md](docs/cloud-testing.md)). Tests upload the places they need
  from `tests/fixtures/`, so the experience's own contents don't matter. Tests that would
  hit the rate limit together run serially.
- **End-to-end cloud** (`cargo test --test cloud -- --ignored`): runs the existing
  end-to-end scripts with `--cloud`. CI runs it with repository secrets on pushes to
  `main` and manual runs only, never on pull requests.
- **Parity suite** (`cargo test --test parity -- --ignored`): a set of scripts (print,
  warn, error, compile error, yield, spawned-thread error, return values, `--arg`,
  `--input`, binary output) run on both hosts. Event streams are normalized (drop `seq`,
  `other` output, timing) and compared. Every allowed difference is listed in the test
  with a link to `docs/cloud-behavior.md`. At 5 task creations per minute (§5.1), one
  task per script makes the suite slow, so the cloud side runs several scripts per task
  where the scripts allow it.
- **Behaviour re-checks** (`tests/cloud_behavior.rs`, probes in `tests/cloud_probes/`):
  one check per finding in [docs/cloud-behavior.md](docs/cloud-behavior.md), to rerun after
  Roblox changes the service. These exist and all pass.

## 13. Effect on the template

- §5.0: CI runs `studio-run run tests/run.luau --place build/test.rbxl --cloud` on Linux
  runners; rocale-cli leaves `foreman.toml`.
- Agents use the same command with or without `--cloud`; non-Windows agents always pass
  it.
- place-promote's `exec` and `test` (specs/place-promote.md §4.3–4.4) could delegate to
  `studio-run --cloud --place-version`, leaving place-promote with only upload,
  download, save and publish. Decide once both exist.

## 14. Open questions

Checked 2026-11-22 against the Open Cloud Luau Execution reference
(create.roblox.com/docs/cloud/reference/features/luau-execution) and the Foreman, Rokit
and Aftman sources.

**Answered:**
- Binary input: one per task, ≤ 100 MiB, valid for 15 minutes and reusable in that
  window; the script gets it as `({...})[1].BinaryInput` (§4.3).
- Return values: `output.results`, JSON-serialized, ≤ 4 MB in all, or the task fails with
  `OUTPUT_SIZE_LIMIT_EXCEEDED`. With binary output enabled, the script must return only a
  `{ BinaryOutput: buffer, ReturnValues: {...} }` table; binary output ≤ 256 MiB, fetched
  from `binaryOutputUri` within 15 minutes (§7.3, §7.4).
- Logs: `GET …/tasks/{id}/logs`, `FLAT` or `STRUCTURED` (with message type and time),
  pages of up to 10,000. Documented as readable after completion; only the last 450 KB
  per task is kept (§7.3).
- Task states: `QUEUED`, `PROCESSING`, `COMPLETE`, `FAILED`, `CANCELLED`. Error codes:
  `SCRIPT_ERROR`, `DEADLINE_EXCEEDED`, `OUTPUT_SIZE_LIMIT_EXCEEDED`, `INTERNAL_ERROR`.
  Tasks and logs are kept 24 hours.
- Limits: 10 incomplete tasks per place; per key owner, 5 task and 5 binary-input
  creations per minute, 200 task reads and 45 log reads per minute (§5.1, §9).
- Timeout: default and maximum 5 minutes (§4.1).
- Scopes: `universe.place.luau-execution-session:read` / `:write`, and
  `universe-places:write` for the legacy place version upload (§5.1).
- Environment: physics doesn't run, place scripts don't auto-run, modules can be required,
  DataModel changes are local to the task, DataStores are real (§6).
- Release assets: §11.
- Local limits, measured with studio-run 0.1.0 and Studio 0.748.0.7481526 on Windows 11
  (one run each, so treat the times as rough):
  - **Embedded input:** a script holding a string literal of 1, 4, 16, 64 and 256 MB ran in
    7.5–10 s each, the same as an empty script. No limit was found, so local `--input` is
    capped at the cloud's 100 MiB (§4.3).
  - **Turning embedded input into a buffer**, 16 MB of random bytes: base64 decoded by a
    buffer-based Luau decoder took 3.2 s; a literal escaped like `wrapper::lua_string`
    (48 MB of source) became a buffer via `buffer.fromstring` in 0.003 s, with the bytes
    checked, and the whole run took 7.9 s, the same as an empty script (§4.3).
  - **Output through Studio's log:** 512-byte `print` lines, all received intact. An empty
    script took 7.4 s. 1 MB of lines took 7.7 s, 4 MB 11–14 s, 16 MB 13.5–15.5 s, and
    64 MB 155 s. Local `--binary-output` is capped at 8 MiB, about 11 MB of base64 lines
    (§4.3). The same slowdown applies to scripts that simply print tens of MB.

- **Probed in real tasks** on 2026-11-23 ([docs/cloud-behavior.md](docs/cloud-behavior.md),
  re-checked by `tests/cloud_behavior.rs`):
  - No `loadstring`; `setfenv`, `MessageOut` and `ScriptContext.Error` work; no Plugin
    capability; `RunService` reports a non-running server. Compiling the user's source as
    a ModuleScript works and replaces inlining (§7.1).
  - Chunk name `TaskScript`. Compile and runtime errors in the task script both fail the
    task with `SCRIPT_ERROR` and `TaskScript:<line>: …`; under §7.1 that only happens for
    wrapper bugs.
  - HttpService follows the experience's Allow HTTP Requests setting.
  - Logs read empty until the task finishes; a timed-out task's logs keep its output.
  - No cancel call: `PATCH`, `POST …:cancel` and `DELETE` all return 404 (§9).
  - Timeouts over 300 s are refused with `400 INVALID_ARGUMENT` (§4.1).
  - Binary input uploads with `PUT`; the task's only argument is `{ BinaryInput = buffer }`;
    binary output round-trips.

  - **Saved versions run tasks**: the legacy upload with `versionType=Saved` returns the
    `versionNumber`, and a task on it loads the uploaded file. A task without a version
    runs on the latest version, Saved included. The upload rejects `.rbxlx` with a trailing
    newline, so studio-run trims it (§4.1).

**Still open:**
1. Place upload. The legacy `POST /universes/v1/{u}/places/{p}/versions?versionType=Saved`
   is marked experimental but returns the `versionNumber` a task needs. The documented
   replacement, `PATCH /assets/v1/assets/{placeId}`, returns a `revisionId` via a
   long-running operation; whether that equals the place version number isn't stated. The
   legacy endpoint works today, so v0.2 uses it.
2. Whether a task can read the `Source` of ModuleScripts that are part of the place, for
   `--trace-modules` (§7.2).

## 15. Milestones

- **Spike, before v0.2.0:** done. `tests/cloud_behavior.rs` answered §14 except two
  questions that don't block v0.2.0, the ModuleScript approach replaced inlining (§7.1),
  and `--place` can upload Saved versions as planned.
- **v0.2.0:** `--cloud` on an existing place version and with `--place` upload; same
  output and exit codes; config file; `doctor --cloud`; Windows, Linux and macOS builds.
  Linux is the point of the release (§13): with the local host behind `#[cfg(windows)]`,
  the other builds are mostly a release-workflow change.
- **v0.2.x:** `--arg`.
- **v0.3.0:** `--input` and `--binary-output` on both hosts; parity suite in CI.
