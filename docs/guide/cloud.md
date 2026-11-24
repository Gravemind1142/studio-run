# Running in the cloud (`--cloud`)

With `--cloud` (or `STUDIO_RUN_HOST=cloud`), the script runs as an
[Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)
task in a place you own, instead of in local Studio. Output, `--format`,
`--output` and exit codes are the same. It works on Windows, Linux and macOS,
with no Studio needed, so it suits CI.

```sh
studio-run run smoke-test.luau --cloud --universe-id <id> --place-id <id>
```

## Setup

1. **An experience for the runs.** Tasks use the experience's **real**
   DataStores and other cloud data, so use one that exists only for this, not
   your game. An empty Baseplate published from Studio is enough. If you run
   jest-roblox, or anything else that loads code with `loadstring`, turn on
   **ServerScriptService → LoadStringEnabled** in the place before you publish
   it. A task has `loadstring` only when the place allows it. Local runs don't
   need this.
2. **An API key** from the
   [Creator Dashboard](https://create.roblox.com/dashboard/credentials),
   restricted to that experience, with:
   - **Luau execution sessions**: read and write
     (`universe.place.luau-execution-session:read`/`:write`). That's all it
     needs: studio-run never uploads or publishes places.
   - **Accepted IP addresses**: your IP. CI runners such as GitHub's have
     changing IPs and need `0.0.0.0/0`.
3. Put the key in the `ROBLOX_API_KEY` environment variable (or another,
   named with `--api-key-env`). The key is never accepted as an argument, so it
   can't end up in shell history or CI logs.
4. Name the place with `--universe-id` and `--place-id`. In CI, pass them
   from secrets.
5. Check it: `studio-run doctor --cloud --universe-id <id> --place-id <id>`
   runs a trivial task. It warns if the place doesn't allow `loadstring`.

## Options

The cloud-only options are `--place-version`, `--queue-timeout`,
`--poll-interval` and `--api-key-env`. They're in the
[command-line reference](../reference/cli.md#run), along with how the shared
options (such as `--timeout`) behave under `--cloud`.

## Why no `--place`

`--place <file>` is a usage error under `--cloud`. The script runs on the
place as you've published it: studio-run doesn't upload place files. Every
upload would add a version to the place, and it would become what Studio and
Team Create open next. To test a place file, publish it (or a copy of the
experience) from Studio, then run on it. The reasoning is in
[design.md](../internals/design.md#no-place-uploads).

## Differences from local runs

The task is a headless server, not Studio's edit mode. What we found is in
[cloud-behavior.md](../internals/cloud-behavior.md); in short:

- **Output arrives when the task finishes**, not while it runs: Roblox only
  makes it available then. With `--output verbose`, text format shows the
  task's progress as `studio-run:` notes.
- No plugins, and no Plugin capability. `loadstring` works only if the place
  has **LoadStringEnabled** on; otherwise it errors with
  `loadstring() is not available`, and studio-run adds a note saying how to
  turn it on. `RunService:IsServer()`
  is true; `IsRunning()` and `IsEdit()` are false. The place's own scripts
  don't run and physics doesn't step.
- HttpService follows the experience's **Allow HTTP Requests** setting.
- `script` is `nil`, as locally. Errors and tracebacks point at your file and
  line, as locally.
- `--input` files are uploaded, packed together, as the task's binary input
  before the task is created; the binary output comes back with the task's
  output.
- Your script is compiled as the body of a function, so a compile error at
  the very end of the file (an unfinished statement) is reported one line
  past it, at `end`, under `--cloud`. With `--trace-modules`, the same goes
  for required modules.
- The task script, with your source and `--arg` values embedded, is limited
  to 4 MB.

## Limits

- `--timeout` is at most 300 seconds, Roblox's limit.
- **Tasks can't be cancelled.** If studio-run gives up waiting (or you press
  Ctrl+C), the task keeps running until it finishes or reaches its timeout.
  studio-run says so and names the task.
- Roblox allows 5 task creations per minute per account (across all your keys)
  and 10 unfinished tasks per place. studio-run retries rate-limited requests
  until `--queue-timeout` runs out.

## Failures

When a cloud run fails with exit code 125, studio-run reports why: the API
refused a request (key, permissions, IDs), or the task failed without the
script reporting a result. With `--format jsonl`, an `error` event carries a
stable `code` such as `auth` or `rate_limited`: see the
[event reference](../reference/events.md#fields).
