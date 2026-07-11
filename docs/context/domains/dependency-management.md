# Dependency Management

## Design

- Dependencies are declared in `yar.toml` using short alias names and git URLs.
- Exact versions are pinned via `yar.lock` with full commit SHAs and content hashes.
- There is no central registry. All dependencies are fetched from git repositories.
- No semver range resolution. Users pin exact tag, rev, or branch.
- Local path dependencies are supported for development.

## Manifest (`yar.toml`)

- `[package]` section: `name` (required), `version` (optional, informational).
- `[dependencies]` section: each key is an alias that becomes the import path
  segment. Each value specifies either `git` + one of `tag`/`rev`/`branch`, or
  `path` for a local source.
- Alias names must be valid Yar import path segments: `[a-zA-Z_][a-zA-Z0-9_]*`.
  The alias `std` is reserved for the compiler-owned standard library.

## Lock File (`yar.lock`)

- Auto-generated TOML dependency graph with explicit `version = 1`.
- Each `[[package]]` node records `name`, `git`, exactly one source ref
  (`tag`, `rev`, or `branch`), `commit` (full 40-character lowercase SHA),
  `hash` (SHA-256 of directory tree paths and file contents excluding
  `.git/`), and zero or more `[[package.dependencies]]` child edges.
- Each child edge records the dependency alias, git URL, and exact ref kind and
  value. The target package node carries the resolved commit and content hash.
- Local `path` dependencies are not written to the lock file.
- Package nodes and child edges are sorted by alias for deterministic output.
- Only lock graph version 1 is accepted.

## Graph Reconciliation

- Before compilation reads dependency caches, and before `yar fetch` performs
  network or cache work, `yar.toml` and `yar.lock` must describe one exact
  graph.
- Git dependencies declared by the root manifest and manifests of its root
  path dependencies must match lock nodes by alias, git URL, ref kind, and ref
  value.
- Every child edge must match its target node by the same full source/ref
  tuple. Duplicate aliases or edges, missing nodes, dependency cycles, and
  unreachable lock nodes are rejected.
- Lock v1 has global alias/source uniqueness. Reusing an alias with a different
  git source or ref is a conflict even across owners. Distinct owner-local
  sources behind one alias require a newer lock schema.
- Local path dependencies may be declared only in the root manifest and remain
  live filesystem inputs. Their manifests may contribute git roots, but may
  not declare another path dependency. A locked git package may not declare
  path dependencies.
- Alias visibility is owner-scoped after reconciliation. Entry and root path
  origins see their own manifest aliases; a locked git origin sees only its
  child edges. Reachability does not grant another origin access to an alias.

## Resolution Order

1. A `std/<package>` path resolves only to an embedded stdlib `PackageId`.
2. Otherwise, reuse an already loaded `PackageId` when applicable.
3. Check the importer's own source tree, including its self alias.
4. Check an alias declared by that origin, resolving it to a path or verified
   git source.
5. Report an error.

Own-origin packages shadow declared dependencies for non-`std` paths. A
selected dependency alias is authoritative, and a missing local path dependency
fails without stdlib substitution. Embedded stdlib imports also use `std/...`
and never consult project-local or external dependency sources. An unresolved
bare known stdlib name receives a migration diagnostic.

## Fetching

- Lock generation and updates resolve declared tags, branches, or revisions.
  For each missing cache entry, `yar fetch` instead initializes a temporary
  repository and requests the locked 40-hex commit directly with a shallow,
  no-tags fetch; mutable ref data never selects content for an existing lock.
- Dependencies are published to a global cache at
  `$HOME/Library/Caches/yar/deps/` on macOS or `$HOME/.cache/yar/deps/` on
  other supported hosts (overridable via `YAR_CACHE`).
- Cache layout: `{cache}/{urlHash16}/{commitSHA}/` where `urlHash16` is the
  first 16 hex characters of SHA-256 of the git URL.
- The `.git` directory is stripped after checkout.
- `yar fetch` verifies existing entries before reporting success. Fresh locked
  checkouts must match the requested commit, content hash, and manifest edges
  in temporary storage before publication at the final cache path.
- When the effective graph has no git roots, `yar fetch` succeeds without a
  lock file or dependency-cache work.
- The dependency index stores sources and alias bindings per owner origin. When
  resolution selects a locked dependency, the loader verifies its cache tree before returning the
  path or parsing source, then verifies that the selected package manifest's
  git dependencies exactly match the node's child edges. Missing, unreadable,
  symlinked, hash-mismatched, or edge-mismatched selected entries stop
  compilation with repair guidance. Unused dependencies and dependencies
  shadowed by local packages do not require a cache or cached-manifest read.
- Cached git trees may contain only real directories and regular files.
  Symlinks and special filesystem entries are rejected.
- Lock generation hashes the fresh checkout. If the same commit already has a
  different cached tree, generation fails instead of recording the cached
  content as trusted.

## Transitive Dependencies

- Each dependency may contain its own `yar.toml`.
- The resolver walks transitive dependencies recursively.
- Diamonds are valid when every use of an alias has the same git URL and exact
  ref. Lock v1 rejects the same alias with a different source/ref tuple.
- Dependency cycles are detected and reported.
- Path dependencies are supported only in the root `yar.toml`. A root path
  dependency's manifest may contribute git dependencies, but may not declare
  another path dependency. Locked git packages may not declare them either.

## CLI Commands

- `yar init` creates a `yar.toml` with `[package]` section.
- `yar add <alias> <git-url> --tag=v1.0.0` adds a dependency and updates
  `yar.lock`.
- `yar remove <alias>` removes a dependency and updates `yar.lock`.
- `yar fetch` reconciles `yar.toml` with `yar.lock`, then downloads and verifies
  every locked dependency.
- `yar lock` regenerates `yar.lock` from `yar.toml`.
- `yar update` re-resolves the full graph. `yar update <git-alias>` replaces
  that dependency's reachable subgraph, preserves unrelated nodes still needed
  by other roots, merges compatible shared aliases using the updated
  resolution, and prunes nodes that become unreachable. Selective update
  refuses a stale unrelated root instead of writing an incoherent graph.
- `yar update <path-alias>` is rejected because a path dependency has no
  independent locked revision; run `yar lock` to reconcile the full graph.
- The Rust CLI under `crates/yar-cli` supports `init`, `add`, `remove`,
  `fetch`, `lock`, and `update` for local path and git dependencies. The Rust
  package loader can consume local path dependencies and locked cache paths
  during package loading.

## Infrastructure

- `crates/yar-compiler/src/manifest.rs` is the Rust manifest, lock, cache,
  fetch, hash-verification, and recursive dependency resolver implementation.
- `crates/yar-compiler/src/lock_graph.rs` validates and reconciles lock graphs,
  verifies selected manifests against recorded edges, and merges selective
  updates.
- `crates/yar-compiler/src/package.rs` builds origin-scoped source and alias
  records from manifests and locked child edges, then verifies a selected
  source during package resolution.

## Constraints

- `yar fetch` requires `git` and network access only when a locked cache entry
  is missing; valid cached entries are verified offline. Lock-generating and
  update commands require both to resolve declared refs. Path-only graphs need
  neither.
- If the remote cannot provide a missing locked object, fetch fails without
  falling back to the recorded tag, branch, or revision. `yar lock` or
  `yar update` is an explicit version change.
- The `YAR_CACHE` environment variable overrides the default cache directory.
- Local `path` dependencies are live filesystem inputs and are not hashed.
- Locked git packages cannot contain `path` dependencies.
- Corrupt cache entries are not repaired or deleted automatically; commands
  fail without publishing or trusting their contents.
- No version range negotiation or automatic resolution. All versions are exact.
- Branch-pinned dependencies are non-reproducible across machines unless the
  lock file is committed and kept up to date.
