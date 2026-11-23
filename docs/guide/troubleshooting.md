# Troubleshooting and limitations

## Check your setup

```sh
studio-run doctor
```

This finds Studio and runs a test script in it. `studio-run doctor --cloud
--universe-id <id> --place-id <id>` instead checks your cloud settings and
key and runs a trivial task.

## Studio isn't found, or the run hangs

- If studio-run says it can't find Studio, install it, or pass `--studio` or
  set `STUDIO_EXE`.
- Studio has to be **logged in at least once**: studio-run reuses your saved
  login. A run that can't log in is likely to hang until `--timeout`.
  studio-run then points out the login problem and the Studio log.
- Exit code 125 means studio-run couldn't run the script at all, or Studio
  exited without the script reporting a result. Rerun with
  `--output verbose`, or `--visible` to watch Studio.
- `--keep-temp` keeps the generated wrapper and output file for inspection.

## Known limitations

- On macOS, hiding Studio means its `--background` flag. That keeps the window
  out of the foreground today. It is not a private desktop, and a later Studio
  build could show a window. Re-run the behaviour checks after a Studio update.
- Local runs need Windows or macOS. On Linux, only `--cloud` works.
- Plugin output shows up in the `other` group. There's no way to tell plugin
  output apart from your modules' output.
- Not tested: running while you have Studio open yourself, or after your saved
  login expires.
- Relies on Studio's log file format to stream output live. If that changes,
  output still arrives, but only once Studio exits.
- `--quitAfterExecution` occasionally leaves Studio at its start page instead
  of quitting. studio-run stops Studio itself after `--grace` seconds.

For `--cloud`, see [its limits](cloud.md#limits).

## How it works

If you want to know why studio-run behaves as it does, [design.md](../internals/design.md)
explains the generated wrapper, how output and events are read from Studio's
log, and how Studio is hidden and stopped. What we know about Studio's CLI is
in [studio-behavior.md](../internals/studio-behavior.md).
