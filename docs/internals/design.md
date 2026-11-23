# Design notes

Why studio-run works the way it does. The Studio behaviour these decisions
rest on is in [studio-behavior.md](studio-behavior.md). The cloud host is
[at the end](#the-cloud-host---cloud).

## Hiding Studio

On Windows, Studio is started on a private desktop (`CreateDesktopW`, then
`STARTUPINFOW.lpDesktop = "WinSta0\<name>"`). It renders and runs normally
there, but the desktop is never shown, so no window appears and focus is never
taken. Each run gets its own desktop, so runs can go in parallel.

It is started suspended, put in a job object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, then resumed. Closing the job takes down
Studio and every process it started, including when studio-run itself is
killed.

Windows finds Studio via the `roblox-studio` protocol handler in the registry,
which Studio's installer keeps pointing at the current version, with the
newest `Versions\*\RobloxStudioBeta.exe` as a fallback.

On macOS, Studio has its own `--background` flag, so there is no private
desktop. Hidden launches pass that flag. Studio is started in a new process
group, and a timeout or studio-run exiting sends `SIGKILL` to the group, which
is the stand-in for the job object. Studio is
`/Applications/RobloxStudio.app` (or the same bundle under `~/Applications`).
`--background` is what Studio does today: it keeps the window out of the
foreground. It is not a separate desktop, and a later Studio build could
change it. What the installed Studio actually did is recorded in
[studio-behavior.md](studio-behavior.md).

## Every run goes through a generated wrapper

studio-run builds Studio's command line itself, and there's no way to pass
your own `--runScriptFile` to `run`. The script is embedded in a wrapper
generated from [src/wrapper.luau](../../src/wrapper.luau), as a `\ddd`-escaped
string literal, which works for any bytes, unlike a long-bracket string.
(`studio-run raw` launches Studio with no wrapper, for the behaviour checks.)

The wrapper exists because Studio on its own:

- exits 0 whatever the script does, and loses uncaught errors raised after
  the script has yielded;
- mixes the script's output with plugin noise and its own echo of the source;
- doesn't always quit after the script.

## Output categories

The script's own output is captured exactly by compiling it with
`loadstring(src, "=file.luau")` (so errors show the right file and line) and
running it with its own `print`/`warn` via `setfenv`. Uncaught errors are
caught with `xpcall`. Everything else reaches the wrapper through
`LogService.MessageOut` with a level, and is reported as `other`.

Structured log messages (`LogService:Info(message, context)`, `Logger`s) also
arrive through `MessageOut`, which passes their context table as a third
argument, already flattened by Roblox
([studio-behavior.md](studio-behavior.md#structured-logging)). A non-empty
context is reported as the event's `context`. Nothing in `MessageOut` says
whether the script or something else logged the message, so these are always
`other`, even when the script logged them.

Errors in threads the script spawned come from `ScriptContext.Error`. They're
matched to the script by chunk name, and the copy of the same error that
`MessageOut` also delivers is dropped.

## Events

The wrapper prints events as short JSON lines starting with a sentinel. Studio
writes them to its log (`%LOCALAPPDATA%\Roblox\logs` on Windows,
`~/Library/Logs/Roblox` on macOS), which studio-run reads while Studio runs,
since the `--outputFile` is only written at the end. The output file is read
as a fallback once Studio exits.

Studio's log cuts lines at about 1,000 characters, so events are split into
pieces of at most 600 bytes with a `more` flag. A context is sent as JSON text
in `contextJson`, so it can be split the same way, after the message's
pieces; studio-run parses it into `context` once the pieces are joined. Each
event has a `seq` so
pieces can be joined back together and duplicates dropped: every event can
arrive twice, once from the live log and once from the output file read at
exit. The sentinel is built from two pieces in the wrapper source, because
Studio echoes that source into its log.

## `result` ends the run

`result` is always the last event, and later output (plugin noise while Studio
shuts down) is dropped. After `result`, Studio gets a few seconds to quit, then
it's killed. This works around `--quitAfterExecution` being unreliable.

## No runner-specific code

A test runner is just a script. The script's return value sets the exit code;
this replaces `ProcessService`, which doesn't exist under the CLI.
jest-roblox has its own module loader (`loadstring` + `setfenv`, since
`debug.loadmodule` is unavailable) and its own `print` mocks, so the wrapper
changes only the entry chunk's environment. Jest's output comes through as
`other`, which is shown by default.

## Binary input and output

Scripts get `--input` files as buffers in `ctx.inputs`, and set binary output
with `ctx.setBinaryOutput`, both added by the wrappers to the script context.

Locally, each input is embedded in the wrapper as an escaped string literal,
like the source, and turned into a buffer with `buffer.fromstring`. Studio ran
scripts with literals of 1–256 MB in the same 7.5–10 s as an empty one, and
`buffer.fromstring` on a 16 MB literal took 3 ms, against 3.2 s to decode the
same data from base64 in Luau. `@` is escaped too, so an input containing the
event sentinel can't inject events through Studio's echo of the wrapper. The
cap is 100 MiB, the cloud's.

The output has to come back through Studio's log, so it's base64-encoded
(URL-safe alphabet, which JSON never escapes) and sent as `binary` events of
512 characters, joined with `more` like split messages and sent just before
`result`. studio-run collects them and never prints them. Lines of 512 bytes
totalling 16 MB added about 6 s to a run, but 64 MB took 155 s, hence the
8 MiB cap.

Under `--cloud`, a task takes one binary input, so the inputs are packed into
one buffer with a small index (`SRI1`, then each name and its data, with
lengths), uploaded with `PUT`, and unpacked by the cloud wrapper from the
task's `BinaryInput` argument. The binary input is created only when there are
inputs. The output goes in the task's binary output after the events, with a
header giving the length of each (`SRB1`, then the two lengths). The two
together must fit in 256 MiB; if they don't, the wrapper drops the output and
fails the run with an error. If the binary output can't be downloaded, the
events still come from the logs, but the run exits 125 when `--binary-output`
was asked for.

## ModuleScripts

Modules loaded with the real `require` print with the real `print`, so their
output is `other`. There's no way around that short of loading them ourselves
(see the module-environment findings): `--trace-modules` does that, and it's
opt-in because it isn't the real `require`.

## The cloud host (`--cloud`)

`--cloud` runs the script as an Open Cloud Luau Execution task instead, and
gives back the same events and exit codes. The behaviour these decisions rest
on is in [cloud-behavior.md](cloud-behavior.md), re-checked by
`tests/cloud_behavior.rs`. The code is in `src/cloud/`; the event printer and
exit codes (`src/output.rs`) are shared with the local host.

### A native client, not rocale-cli

studio-run calls the Open Cloud API itself (`src/cloud/api.rs`) rather than
shelling out to rocale-cli. rocale-cli's output and exit codes aren't
documented, so mapping them onto studio-run's would be guesswork; studio-run
needs the exact task request (timeout, binary output) and its result; and it
avoids a second tool and release cadence. The client only needs to create and poll a
task, read its logs and download its binary output.

### The user's source becomes a ModuleScript

Tasks have no `loadstring`. The cloud wrapper
([src/cloud/wrapper.luau](../../src/cloud/wrapper.luau)) creates a ModuleScript
named after the file, sets its `Source` to `return function(...) <source>\nend`
with the source starting on line 1, and requires it. Then it does what the
local wrapper does with `loadstring`'s result: `setfenv` for `print`/`warn`,
and `xpcall` with the script context as the argument.

The context (`ctx.host`, `ctx.args`, ...) is an argument rather than a global
such as `STUDIO_RUN`: the user's code is a ModuleScript with its own
environment, which makes a global awkward, while an argument works the same
way on both hosts. It's embedded in the script as a table
constructor; under `--cloud`, `place.version` is filled in from
`game.PlaceVersion` when the task runs, since the latest version isn't known
when the task is created.

This beat the alternative, inlining the source into the task script:

- Errors and tracebacks name the user's file and line with no remapping. The
  only other frames are the wrapper's (`TaskScript:<n>`), which are dropped as
  the local wrapper drops its `RunScript:` frames.
- The user's code is its own chunk, so the wrapper's locals aren't in scope
  for it.
- A compile error doesn't fail the task. `require` only says the module failed
  to load, but the real message arrives through `MessageOut`, so the wrapper
  reports it as a `compile` result, as locally. Otherwise the task would fail
  with a `TaskScript:` message and no result.

The cost: the source is a function body, so a compile error at the very end
of the file is reported one line late, at the wrapper's closing `end` rather
than at `<eof>`. (Top-level `export type` still compiles there.) Requiring the
source verbatim would avoid that, but a module must return exactly one value
and gets no `...`.

`--trace-modules` loads required ModuleScripts the same way, where the local
wrapper uses `loadstring`. For each one, the traced `require` reads its
`Source` (a task can read the place's own modules), compiles it as a new
ModuleScript named after the original's full name (so errors read
`ReplicatedStorage.Greeter:8`, as locally), `setfenv`s the returned function
with the captured `print`/`warn` and `script` set to the original, calls it
once and caches the result.

### Events come back in the binary output

Task logs can't be read until the task finishes, and keep only the last
450 KB, dropping the start of a long run. So the wrapper collects its events
and returns them, as the same `@@studio-run@@{json}` lines, in the task's
binary output (up to 256 MiB), followed by the script's own binary output
([above](#binary-input-and-output)). The result is also returned as the task's
return value, in case the events are lost. The wrapper prints each event too,
so a task that times out (whose logs keep the output from before the timeout)
still yields what it printed. studio-run reads the binary output, falls back
to the logs, and feeds either to the same parser and printer as a local run.
In the logs, every engine message also appears unwrapped; those lines are
ignored, since the wrapper already reported them as `other`.

Output therefore arrives when the task finishes, not while it runs.

### Deadlines, retries, no cancelling

`--timeout` is the task's own timeout: how long the script may run, at most
300 s. Waiting for the task to start gets `--queue-timeout` on top, and
studio-run gives up when both have passed (exit 124).

429s and 5xx are retried with backoff, respecting `retry-after`, until that
deadline. A failed task creation never created a task, so it's safe to
retry.

Tasks can't be cancelled. When studio-run gives up or is interrupted, the
task runs on until it finishes or reaches its timeout. studio-run says so and
names the task.

### No place uploads

Tasks run on a version of a place that already exists: its latest, or
`--place-version`. `--place <file>` is a usage error under `--cloud`.

Running a local place file would mean uploading it as a new place version
first, and every run would add one. That fills the place's version history,
and the upload becomes the place's latest version, which is what Studio and
Team Create open next. Neither upload API avoids it:

- The legacy place-versions endpoint (`POST /universes/v1/…/versions`) is
  marked experimental in Roblox's docs.
- The Assets API (`PATCH /assets/v1/assets/{placeId}`) is beta. Its
  permission can't be restricted to one experience, so a key with it can change
  every asset on the account.

So publishing a place to run on is left to the user, in Studio, and the key
needs nothing beyond Luau execution sessions. What we found about both upload
APIs before deciding this is in
[cloud-behavior.md](cloud-behavior.md#place-versions) (issue #7).

### One checksum file per release

Releases publish a single `SHA256SUMS` rather than a `.sha256` per zip.
Foreman matches asset names by substring and takes the first match in the
release's asset list, so `studio-run-…-linux-x86_64.zip.sha256` also matches
`linux-x86_64`, and Foreman would try to install it if GitHub listed it before
the zip. Both macOS zips are always shipped, because Foreman's Intel macOS
keywords end with a bare `macos` and would pick the `aarch64` zip if it were
the only one. These rules come from reading the Foreman, Rokit and Aftman
sources.
