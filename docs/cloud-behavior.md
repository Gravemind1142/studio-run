# How Open Cloud Luau Execution behaves

The `--cloud` host ([design.md](design.md#the-cloud-host---cloud)) runs scripts
as [Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)
tasks. The API reference leaves a lot out. This file records what we found by
running tasks, how we found it, and which check in
[tests/cloud_behavior.rs](../tests/cloud_behavior.rs) re-tests it. It's the
cloud counterpart of [studio-behavior.md](studio-behavior.md).

Last checked: engine **0.748.0.7481528** (`version()` inside a task),
2026-11-23.

## Re-checking

Set up a test experience and key ([cloud-testing.md](cloud-testing.md)), then:

```powershell
cargo test --test cloud_behavior -- --ignored --nocapture --test-threads=1
```

This takes about two and a half minutes, mostly waiting out the task creation
rate limit. Each test runs probes from `tests/cloud_probes/` and checks the
findings below; with `--nocapture` it also prints what it saw as `FINDING`
lines. A failure means Roblox has changed something: update the finding and
"Last checked", and studio-run (and [design.md](design.md)) if they relied on
the old behaviour.

## How we investigated

Each probe is a Luau script run as a task. Most return one table describing
what they saw, which comes back JSON-serialized in the task's `output.results`.
The tests read the task resource, its logs (`STRUCTURED` view) and its binary
output with a minimal client in `tests/common/cloud.rs`.

## Findings

### Environment

| Finding | How we know | Check |
|---|---|---|
| **`loadstring` isn't available**: calling it errors with `loadstring() is not available`. `setfenv`/`getfenv` work. | `environment.luau`. | `environment` |
| `RunService` reports a server that isn't Studio and isn't running: `IsServer` true; `IsClient`, `IsStudio`, `IsEdit`, `IsRunning`, `IsRunMode` false. | `environment.luau`. | `environment` |
| Plugin APIs aren't available: `plugin` is nil, and `ChangeHistoryService:SetWaypoint` errors with "lacking capability Plugin". `CoreGui` isn't reachable (`GetService` returns nil) and `StudioService` isn't a valid service. | `environment.luau`. | `environment` (plugin, ChangeHistoryService) |
| `LogService.MessageOut` and `ScriptContext.Error` can be connected to. | `environment.luau`. | `environment` |
| The task script's chunk name is **`TaskScript`**: in `debug.info`, error messages (`TaskScript:<line>:`) and tracebacks. | `environment.luau`, `runtime_error.luau`. | `environment`, `runtime_error` |
| `script` is nil, and without a binary input the task gets no arguments (`select("#", ...)` is 0). | `environment.luau`. | `environment` |
| `game.PlaceId` and `game.GameId` are the test place and universe; `game.PlaceVersion` is the version the task ran on, which is also in the task's `path`. | `environment.luau`, task paths. | `environment` |
| HttpService follows the experience's **Allow HTTP Requests** setting (off by default): with it off, `GetAsync` errors with "HTTP requests are not enabled. Enable via Experience Settings". `JSONEncode` works regardless. | `environment.luau` on a new experience. | — (depends on the experience) |
| The engine is the same build as Studio's (`version()` 0.748.0.7481528 against Studio 0.748.0.7481526). | `environment.luau`. | — |

### Compiling the user's source as a ModuleScript

The cloud wrapper can't use `loadstring`, so it creates a ModuleScript named
after the user's file, sets its `Source` to `return function(...) <source>\nend`
(the source starts on the module's line 1) and requires it.

| Finding | How we know | Check |
|---|---|---|
| A ModuleScript created by the task, with `Source` set and no parent, can be required. | `environment.luau`, `module_loading.luau`. | `environment`, `module_loading` |
| **Errors and tracebacks use the module's `Name` and the source's own line numbers**: `tests/runtime.luau:3: boom xy`, traceback `tests/runtime.luau:3` followed only by the wrapper's `TaskScript:<n>` frames. | `module_loading.luau`. | `module_loading` |
| **A compile error doesn't fail the task.** `require` raises only "Requested module experienced an error while loading"; the real message (`tests/compile.luau:2: Expected identifier …`) arrives through `LogService.MessageOut` as an error. | `module_loading.luau`. | `module_loading` |
| `setfenv` on the function the module returns replaces `print` for the user's code; arguments passed to it arrive as `...`. | `module_loading.luau`. | `module_loading` |
| Errors in threads the user's code spawns reach `ScriptContext.Error` with the module's name in the stack (`tests/async.luau, line 2`), the form the local wrapper already matches on. | `module_loading.luau`. | `module_loading` |
| Required verbatim (not wrapped), a module must return exactly one value ("Module code did not return exactly one value"), and top-level `export type` is allowed. | `module_loading.luau`. | `module_loading` |
| **Wrapped, top-level `export type` compiles and runs too**, as it does locally. (Recorded earlier as a compile error; it was never re-checked, and isn't one on engine 0.748.) | `export_type.luau` on both hosts. | `tests/parity.rs` |
| **A compile error at the end of the source is reported one line later**, at the wrapper's closing `end`: `compile_error.luau:3: Expected identifier when parsing expression, got 'end'`, where Studio says `compile_error.luau:2: … got <eof>`. Errors elsewhere in the file have the file's own line numbers. | `compile_error.luau` on both hosts. | `tests/parity.rs` (an allowed difference) |

### Loading the place's ModuleScripts (`--trace-modules`)

`--trace-modules` loads each required ModuleScript as above: a new
ModuleScript named after the original's full name, with the original's
`Source` wrapped.

| Finding | How we know | Check |
|---|---|---|
| **A task can read the `Source` of a ModuleScript that's part of the place.** | `place_modules.luau` on the test place, `modules.rbxlx`. | `place_modules` |
| Loaded under its full name (`ReplicatedStorage.Greeter`), with `print` replaced by `setfenv`, all of the module's output is captured, top level included. Errors and tracebacks read `ReplicatedStorage.Greeter:8`, the same as `loadstring` with that chunk name locally. | `place_modules.luau`. | `place_modules` |
| Names with dots work everywhere the wrapper matches on them: a compile error arrives through `MessageOut` as `ReplicatedStorage.Bad:1: …`, and an error in a spawned thread reaches `ScriptContext.Error` with stack `ReplicatedStorage.Async, line 2`. | `place_modules.luau`. | `place_modules` |

### Results and errors

| Finding | How we know | Check |
|---|---|---|
| Return values come back in `output.results`. `nil` and Instances become `null`; a buffer becomes `{"t":"buffer","base64":…}` and NaN `{"t":"numeric","v":"nan"}`. | `return_values.luau`. | `return_values` |
| **A task script that doesn't compile fails the task**: `FAILED`, `SCRIPT_ERROR`, message `TaskScript:<line>: <error>`, and no logs. | `compile_error.luau`. | `compile_error` |
| An uncaught error fails the task the same way: `SCRIPT_ERROR`, `TaskScript:<line>: <message>`, no traceback in the message. The logs have the output before it and the error with a `Stack Begin` / `Script 'TaskScript', Line <n>` / `Stack End` stack. | `runtime_error.luau`. | `runtime_error` |
| Compile and runtime errors share the error code; only the message tells them apart. | The two above. | `compile_error`, `runtime_error` |

### Timing, logs and cancelling

| Finding | How we know | Check |
|---|---|---|
| **The maximum timeout is 300 s.** `301s` is refused with `400 INVALID_ARGUMENT` ("must be between 1 and 300 seconds inclusive"). | `timeouts`. | `timeouts` |
| A task that runs past its timeout fails with `DEADLINE_EXCEEDED`, and **its logs keep the output from before the timeout**. | `slow.luau` with a 5 s timeout. | `timeouts` |
| **Logs are empty until the task finishes**: read 8 s into a running task that printed in its first second, they're an empty list (HTTP 200). | `slow.luau`. | `logs_while_running_and_cancel` |
| **There's no way to cancel a task**: `PATCH` with `state: CANCELLED` (with and without `updateMask`), `POST …:cancel` and `DELETE` all return 404, and the task runs to the end. | `slow.luau`. | `logs_while_running_and_cancel` |
| Tasks start at once on an idle place (`PROCESSING` on the first poll), and a trivial task completes about 1 s after it's created. | Every task's `createTime`/`updateTime`. | — |
| Each `print` is one log message; `STRUCTURED` gives its type (`OUTPUT`, `WARNING`, `ERROR`) and time, `FLAT` just the text. | `slow.luau`. | `logs_while_running_and_cancel` |

### Binary input and output

| Finding | How we know | Check |
|---|---|---|
| A binary input's `uploadUri` takes the bytes with **`PUT`**. | `binary_input_and_output`. | `binary_input_and_output` |
| With a binary input, the task's only argument is a table whose only key is `BinaryInput`, a buffer. | `binary_io.luau`. | `binary_input_and_output` |
| With `enableBinaryOutput`, returning `{ BinaryOutput = buffer, ReturnValues = {…} }` gives `ReturnValues` in `output.results` and the buffer at `binaryOutputUri`, byte for byte. | `binary_io.luau`. | `binary_input_and_output` |
| With `enableBinaryOutput`, any other return value fails the task: `SCRIPT_ERROR`, "Returned table must contain the BinaryOutput key." | An early `doctor --cloud` that returned a plain table. | — |

### Place versions

studio-run doesn't upload places ([design.md](design.md#no-place-uploads)), so
these findings aren't re-checked. They're a record of what we found on
2026-11-23 while deciding that (issue #7).

| Finding | How we know | Check |
|---|---|---|
| **A version uploaded as Saved (unpublished) can run tasks**: `POST /universes/v1/{universe}/places/{place}/versions?versionType=Saved` returns `{"versionNumber": n}`, and a task on version *n* loads that file. | A probe returning `game.PlaceVersion` on the uploaded `marker.rbxlx`. | — |
| **A task created without a version runs on the latest version, including a Saved one**, not the latest published one. | A probe returning `game.PlaceVersion` after the upload. | — |
| **The upload rejects XML with a newline after `</roblox>`**: `400`, "Invalid Content stream". The same file without it uploads. Indentation, referent format and `NeedsPivotMigration` don't matter. Binary `.rbxl` uploads as `application/octet-stream`. | Uploading variants of `marker.rbxlx` and Rojo 7.7.1 builds. | — |
| The upload needs `universe-places:write`; without it, `403 Scope not authorized`. | A key without it. | — |
| Uploading a file identical to an existing version seems to return that version's number instead of creating a new one. | The same trimmed `marker.rbxlx` uploaded twice returned version 9 both times. Seen once. | — |
| **The Assets API can upload a place too**: `PATCH /assets/v1/assets/{placeId}`, multipart, with `request` `{"assetId": <place>, "assetType": "Place", "published": false}` and the file as `fileContent` (`application/xml` for `.rbxlx`). It returns an operation (`operations/{id}`, `done: false`), done about a second later when polled with `GET /assets/v1/operations/{id}`. | Uploading `marker.rbxlx`. | — |
| **Its `revisionId` is the place version**: a task on version `revisionId` has `game.PlaceVersion` equal to it and loads the uploaded file. Asset versions (`GET /assets/v1/assets/{placeId}/versions`) and place versions are the same list. With `published: false` the result has `isPublished: false`. | A probe returning `game.PlaceVersion` on the revision. | — |
| **It rejects XML with a newline after `</roblox>` too, but later**: the upload returns 200, and the operation finishes with `error` `{"code": "DataLoss", "message": "Place Content Corrupted"}`. | Uploading the untrimmed `marker.rbxlx`. | — |
| It needs the **Assets** API permission (read to poll the operation, write to upload). Without it, every Assets API request, even a `GET` of the place's asset, is `403 PERMISSION_DENIED`, "User not authenticated". `universe-places:write` isn't enough, and the Assets permission can't be restricted to one experience. The key we tested also had `universe-places:write`, so we haven't checked that Assets alone is enough. | A key before and after adding Assets read and write. | — |
| Not checked: whether leaving out `published` publishes the version, and binary `.rbxl` uploads. | — | — |
