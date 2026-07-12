# CI/CD

## Continuous Integration

CI runs on pull requests and pushes to `main`.

The workflow validates the shipped Rust CLI/runtime path and the Rust
workspace:

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` run on Linux.
- `cargo test --workspace` runs on Linux and macOS.
- `./scripts/verify-rust-testdata.sh` builds the Rust CLI and Rust runtime
  once, then verifies that the Rust CLI can native-build every checked-in
  `testdata/**/main.yar` fixture.
- `./scripts/verify-rust-testdata-run.sh` builds the same Rust CLI/runtime
  pair, then executes every success-oriented `testdata/**/main.yar` fixture
  with the Rust runtime. It skips fixtures whose purpose is panic, unhandled
  error, or failing-test behavior, and provides the required argv/env setup for
  the process/environment fixture.
- Linux runners install `clang` explicitly because native Yar build, run, and
  test coverage invokes the compiler through the external C toolchain.
- The GoReleaser snapshot job installs Zig and `cargo-zigbuild`, then validates
  Rust CLI release packaging before changes can pass CI.

The CI token is read-only. Release publishing is not available from the CI
workflow.

## Release CD

Release CD is tag based. Pushing a tag that matches `v*` runs the release
workflow, verifies Rust format/clippy/tests, and then publishes a GitHub
Release through GoReleaser.

The release workflow can also be run manually. Manual runs execute the same
verification and a local GoReleaser snapshot, but they do not publish a GitHub
Release.

Release artifacts package the Rust `yar` CLI plus one target-keyed runtime
bundle for:

- `darwin/amd64`
- `darwin/arm64`
- `linux/amd64`
- `linux/arm64`
- `windows/amd64`

The bundle is stored under `runtimes/<target-triple>/` and contains
`yar-runtime.toml` plus `libyar_runtime.a`. The Windows artifact targets Rust's
`x86_64-pc-windows-gnu` target. Snapshot CI validates every packaged path and
manifest, then uses the extracted Linux AMD64 compiler and its discovered
bundle for a native smoke build without environment overrides. Runtime staging
also checks each manifest's ordered libraries against the target's
`rustc --print native-static-libs` output.
Users still need `clang` available on `PATH` when they use commands that
produce or execute native Yar programs: `yar build`, `yar run`, and `yar test`.

## Release Operator Path

1. Land the change on `main` after CI passes.
2. Create and push a version tag such as `v0.1.0`.
3. Let the release workflow publish the GitHub Release assets and checksum
   file.
4. Use the manual release workflow only for snapshot validation, not publishing.
