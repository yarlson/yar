# Dependency Management

## Design

- Dependencies are declared in `yar.toml` using short alias names and git URLs.
- Exact versions are pinned via `yar.lock` with full commit SHAs and content hashes.
- There is no central registry. All dependencies are fetched from git repositories.
- No semver range resolution. Users pin exact tag, rev, or branch.
- Local path overrides are supported for development via `path` dependencies.

## Manifest (`yar.toml`)

- `[package]` section: `name` (required), `version` (optional, informational).
- `[dependencies]` section: each key is an alias that becomes the import path
  segment. Each value specifies either `git` + one of `tag`/`rev`/`branch`, or
  `path` for local overrides.
- Alias names must be valid Yar import path segments: `[a-zA-Z_][a-zA-Z0-9_]*`.

## Lock File (`yar.lock`)

- Auto-generated TOML file. Each `[[package]]` entry records `name`, `git`,
  version ref, `commit` (full 40-char SHA), and `hash` (SHA-256 of directory
  tree contents excluding `.git/`).
- Local `path` dependencies are not written to the lock file.
- Entries are sorted by name for deterministic output.

## Resolution Order

1. Cache check (already loaded package).
2. Local filesystem (`rootDir/importPath`).
3. Dependency index (first import path segment matched against alias in
   `yar.toml` → cached directory).
4. Embedded stdlib fallback.
5. Error.

Local packages shadow dependencies. Dependencies shadow stdlib.

## Fetching

- Dependencies are fetched via shallow `git clone` to a global cache at
  `os.UserCacheDir()/yar/deps/` (overridable via `YAR_CACHE`).
- Cache layout: `{cache}/{urlHash16}/{commitSHA}/` where `urlHash16` is the
  first 16 hex characters of SHA-256 of the git URL.
- The `.git` directory is stripped after cloning.
- Content integrity is verified by comparing SHA-256 of the directory tree
  against the hash in `yar.lock`.

## Transitive Dependencies

- Each dependency may contain its own `yar.toml`.
- The resolver walks transitive dependencies recursively.
- Diamond dependency conflicts (same alias, different version) are errors
  unless the root `yar.toml` declares an explicit override.
- Dependency cycles are detected and reported.

## CLI Commands

- `yar init` creates a `yar.toml` with `[package]` section.
- `yar add <alias> <git-url> --tag=v1.0.0` adds a dependency and updates
  `yar.lock`.
- `yar remove <alias>` removes a dependency and updates `yar.lock`.
- `yar fetch` downloads all dependencies from `yar.lock` to the cache.
- `yar lock` regenerates `yar.lock` from `yar.toml`.
- `yar update [alias]` re-resolves dependencies and updates `yar.lock`.

## Infrastructure

- `internal/deps/manifest.go` parses and writes `yar.toml`.
- `internal/deps/lockfile.go` parses and writes `yar.lock`.
- `internal/deps/fetch.go` handles git cloning, caching, and hash computation.
- `internal/deps/resolve.go` walks transitive dependencies and detects conflicts.
- `internal/deps/index.go` provides the `Index` type consumed by
  `internal/compiler/packages.go` during package loading.
- `internal/compiler/packages.go` integrates the dependency index into
  `packageLoader` via the `depIndex` field and `loadDepIndex()` function.

## Constraints

- `git` must be available on `PATH` for fetching git dependencies.
- Fetching requires network access. Building with only local/path dependencies
  does not require network access.
- The `YAR_CACHE` environment variable overrides the default cache directory.
- No version range negotiation or automatic resolution. All versions are exact.
- Branch-pinned dependencies are non-reproducible across machines unless the
  lock file is committed and kept up to date.
