# CI/CD

## Continuous Integration

CI runs on pull requests and pushes to `main`.

The workflow validates the Go compiler and the native Yar execution boundary:

- `golangci-lint` runs against the checked-in `.golangci.yaml`.
- `go test -race -count=1 -v -timeout=120s ./...` runs on Linux and macOS.
- Linux runners install `clang` explicitly because native Yar build, run, and
  test coverage invokes the compiler through the external C toolchain.
- A GoReleaser snapshot dry run validates release packaging before changes can
  pass CI.

The CI token is read-only. Release publishing is not available from the CI
workflow.

## Release CD

Release CD is tag based. Pushing a tag that matches `v*` runs the release
workflow, verifies lint and tests, and then publishes a GitHub Release through
GoReleaser.

The release workflow can also be run manually. Manual runs execute the same
verification and a local GoReleaser snapshot, but they do not publish a GitHub
Release.

Release artifacts package the `yar` Go CLI for:

- `darwin/amd64`
- `darwin/arm64`
- `linux/amd64`
- `linux/arm64`
- `windows/amd64`

The artifacts contain the compiler CLI. Users still need `clang` available on
`PATH` when they use commands that produce or execute native Yar programs:
`yar build`, `yar run`, and `yar test`.

## Release Operator Path

1. Land the change on `main` after CI passes.
2. Create and push a version tag such as `v0.1.0`.
3. Let the release workflow publish the GitHub Release assets and checksum
   file.
4. Use the manual release workflow only for snapshot validation, not publishing.
