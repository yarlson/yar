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
  version ref, `commit` (full 40-character lowercase SHA), and `hash` (SHA-256
  of directory tree paths and file contents excluding `.git/`).
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

- Dependencies are fetched via `git clone`, shallow for tags and branches, to a
  global cache at
  `$HOME/Library/Caches/yar/deps/` on macOS or `$HOME/.cache/yar/deps/` on
  other supported hosts (overridable via `YAR_CACHE`).
- Cache layout: `{cache}/{urlHash16}/{commitSHA}/` where `urlHash16` is the
  first 16 hex characters of SHA-256 of the git URL.
- The `.git` directory is stripped after cloning.
- `yar fetch` verifies existing entries before reporting success. Fresh clones
  are checked against the locked commit and hash in temporary storage before
  they are published at the final cache path.
- The dependency index stores lock metadata. When package resolution selects a
  locked dependency, the loader verifies its cache tree before returning the
  path or parsing source. Missing, unreadable, symlinked, or hash-mismatched
  selected entries stop compilation with repair guidance. Unused dependencies
  and dependencies shadowed by local packages do not require a cache.
- Cached git trees may contain only real directories and regular files.
  Symlinks and special filesystem entries are rejected.
- Lock generation hashes the fresh checkout. If the same commit already has a
  different cached tree, generation fails instead of recording the cached
  content as trusted.

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
- The Rust CLI under `crates/yar-cli` supports `init`, `add`, `remove`,
  `fetch`, `lock`, and `update` for local path and git dependencies. The Rust
  package loader can consume local path dependencies and locked cache paths
  during package loading.

## Infrastructure

- `crates/yar-compiler/src/manifest.rs` is the Rust manifest, lock, cache,
  fetch, hash-verification, and recursive dependency resolver implementation
  used by the Rust CLI and package loader.
- `crates/yar-compiler/src/package.rs` builds a Rust dependency index from
  local path dependencies and locked metadata, then verifies a selected cache
  path during package resolution.

## Constraints

- `git` must be available on `PATH` for fetching git dependencies.
- Fetching requires network access. Building with only local/path dependencies
  does not require network access.
- The `YAR_CACHE` environment variable overrides the default cache directory.
- Local `path` dependencies are live filesystem inputs and are not hashed.
- Corrupt cache entries are not repaired or deleted automatically; commands
  fail without publishing or trusting their contents.
- The current flat lock format is still indexed as written. Package loading
  does not reject duplicate package names, prove that every declared git
  dependency has a matching entry, prove that every entry is reachable from
  `yar.toml`, or reconcile locked source tuples with manifest declarations.
- No version range negotiation or automatic resolution. All versions are exact.
- Branch-pinned dependencies are non-reproducible across machines unless the
  lock file is committed and kept up to date.
