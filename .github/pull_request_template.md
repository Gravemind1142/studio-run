## What this changes

<!-- What and why, in a few sentences. Link the issue if there is one. -->

## Checklist

- [ ] `cargo fmt` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] End-to-end tests pass locally (`cargo test --test studio -- --ignored`), or this change doesn't touch the wrapper, event parsing, log reader, launching or run loop
- [ ] `CHANGELOG.md` updated, if users would notice
- [ ] Docs updated (`docs/reference/cli.md` and the relevant guide page), if options, output or exit codes changed
