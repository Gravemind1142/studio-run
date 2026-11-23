# Design notes

Why studio-run works the way it does. The Studio behaviour these decisions
rest on is in [studio-behavior.md](studio-behavior.md).

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
generated from [src/wrapper.luau](../src/wrapper.luau), as a `\ddd`-escaped
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

Errors in threads the script spawned come from `ScriptContext.Error`. They're
matched to the script by chunk name, and the copy of the same error that
`MessageOut` also delivers is dropped.

## Events

The wrapper prints events as short JSON lines starting with a sentinel. Studio
writes them to its log, which studio-run reads while Studio runs, since the
`--outputFile` is only written at the end.

Studio's log cuts lines at about 1,000 characters, so events are split into
pieces of at most 600 bytes with a `more` flag. Each event has a `seq` so
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

## ModuleScripts

Modules loaded with the real `require` print with the real `print`, so their
output is `other`. There's no way around that short of loading them ourselves
(see the module-environment findings): `--trace-modules` does that, and it's
opt-in because it isn't the real `require`.
