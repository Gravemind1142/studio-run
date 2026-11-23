# Releasing

For maintainers.

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
`v0.2.0-rc.1`, becomes a pre-release. The reasons are in
[design.md](design.md#one-checksum-file-per-release).
