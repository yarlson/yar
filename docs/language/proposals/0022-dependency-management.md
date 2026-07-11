# Proposal: Dependency Management

Status: accepted

## 1. Summary

Add git-based dependency management to Yar via `yar.toml` manifest and
`yar.lock` lock file, with no central registry.

The implemented version provides:

- `yar.toml` manifest declaring project metadata and dependencies by alias
- explicit `version = 1` `yar.lock` graph pinning git dependencies to exact
  commit SHAs and content hashes and recording full source/ref child edges
- alias-based import mapping (dependency alias becomes the top-level import
  path segment)
- git-based fetching to a global user cache, with shallow clones for tags and
  branches
- SHA-256 lock hashes verified before cached source is loaded
- temporary verification before newly fetched content is published
- transitive dependency resolution with conflict detection
- exact manifest/lock reconciliation and reachable-closure validation before
  dependency cache or network access
- local path dependencies for development workflows
- CLI commands: `init`, `add`, `remove`, `fetch`, `lock`, `update`
- compiler integration through origin-scoped source and alias bindings

## 2. Motivation

Yar programs cannot reuse code across project boundaries. The current import
system resolves packages only from the local directory tree or the embedded
stdlib. There is no mechanism to declare, fetch, version, or share external
libraries.

Without dependency management:

- every reusable library must be copy-pasted into the project
- there is no way to pin specific versions of shared code
- there is no integrity verification for external code
- library authors cannot publish packages for others to consume

## 3. User-Facing Examples

### Valid examples

**yar.toml:**

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
http = { git = "https://github.com/user/yar-http.git", tag = "v0.3.1" }
json = { git = "https://github.com/user/yar-json.git", rev = "a1b2c3d" }
local_lib = { path = "../my-local-lib" }
```

**Using a dependency in Yar source:**

```yar
package main

import "http"
import "http/router"

fn main() i32 {
    http.get("https://example.com")?
    return 0
}
```

**CLI workflow:**

```bash
yar init
yar add http https://github.com/user/yar-http.git --tag=v0.3.1
yar build .
```

### Invalid examples

```toml
[dependencies]
"my-lib" = { git = "https://example.com/repo.git", tag = "v1.0.0" }
```

Invalid because `my-lib` contains a hyphen. Dependency aliases must be valid
Yar import path segments: `[a-zA-Z_][a-zA-Z0-9_]*`.

```toml
[dependencies]
lib = { git = "https://example.com/repo.git", tag = "v1.0.0", rev = "abc123" }
```

Invalid because exactly one of `tag`, `rev`, or `branch` must be specified.

```toml
[dependencies]
lib = { path = "../lib", git = "https://example.com/repo.git" }
```

Invalid because `path` and `git` are mutually exclusive.

## 4. Semantics

- A `yar.toml` file in the project root declares external dependencies.
- Each dependency has a short alias name that becomes the first segment of its
  import path in source code.
- `yar lock` resolves all git dependencies, clones them, computes content
  hashes, and writes a `version = 1` `yar.lock` graph. Each package node records
  its alias, git URL, exact ref kind and value, resolved commit, content hash,
  and full alias/git/ref edges to its git dependencies.
- Lock generation hashes the fresh checkout. If an existing cache entry for the
  resolved commit differs, lock generation fails instead of trusting that
  cache content.
- Before compilation reads dependency caches, and before `yar fetch` uses the
  cache or network, Yar derives git roots from the root manifest and manifests
  of its root path dependencies, then reconciles them with the lock graph. Root
  declarations and child edges match nodes by alias, git URL, ref kind, and ref
  value.
- Duplicate package aliases or child edges, missing nodes, source/ref
  mismatches, dependency cycles, and unreachable package nodes are errors.
  Missing or unsupported lock versions are rejected with guidance to run
  `yar lock`. Regeneration performs ordinary full resolution, so a moved tag or
  branch can produce a different commit and the lock diff must be reviewed.
- `yar fetch` verifies both existing entries and fresh temporary checkouts
  against `yar.lock`. A fresh entry is moved to its final cache path only after
  its commit and content hash match. Each fetched manifest is also checked
  against its package node's child edges.
- During compilation, the package loader builds origin-scoped source records
  and alias bindings from `yar.toml` and `yar.lock`. It checks same-origin
  packages, then aliases declared by the importing origin, then stdlib.
- The source index stores lock metadata. When resolution selects a locked
  dependency, its cache tree is hash-verified before the path is returned or
  its manifest or source is parsed. The verified manifest's git dependencies
  must then exactly match the node's child edges. Missing, corrupt, or
  edge-divergent selected entries stop package loading; compilation performs no
  cache repair and does not substitute a same-named stdlib package. Unused and
  same-origin-shadowed entries do not require a cache or manifest read.
- Cached git trees contain only real directories and regular files. Symlinks
  and special filesystem entries are rejected. Local path dependencies remain
  live, unhashed filesystem inputs.
- A dependency alias is a binding owned by a source origin, not package
  identity. Packages use `PackageId = (source origin, source-relative subpath)`.
- For each non-stdlib importer, resolution checks same-origin packages, then
  aliases declared directly by that origin, then embedded stdlib.
- Entry aliases come from the root manifest, root path origins use aliases from
  their own manifests, and locked git origins use their lock node's child edges.
- Imports inside embedded stdlib are sealed to the stdlib origin.
- A selected dependency alias is authoritative. A missing declared path fails
  loading instead of falling through to a same-named stdlib package.
- Lock reachability does not grant import visibility. Each importing origin
  must directly declare every external alias it uses. Source that relied on a
  merely reachable transitive alias must add it to that source's manifest.
- Transitive dependencies are discovered by reading `yar.toml` in each
  fetched dependency. Reusing an alias with a different git URL, ref kind, or
  ref value is an error. There is no root override.
- Lock v1 and the cache layout are unchanged. Lock v1 still requires one global
  source/ref tuple per alias, so different owners cannot yet reuse one alias for
  different targets. That requires lock v2.
- `path` dependencies are resolved directly from the filesystem and are not
  written to `yar.lock`. They may be declared only in the root manifest. A root
  path dependency's manifest may declare git dependencies, but may not declare
  another path dependency; neither may a locked git package.
- `yar update <git-alias>` resolves a replacement graph for the selected root,
  preserves unrelated nodes still reachable from unselected roots, merges
  compatible shared nodes using the updated resolution, and prunes orphans. It
  refuses to write if an unselected root is stale or a shared alias conflicts.
  A targeted path update is rejected with guidance to run `yar lock`.

## 5. Type Rules

No new type rules. Dependencies are loaded as ordinary Yar packages through
the existing package loader. All existing type checking, export validation,
and import rules apply unchanged.

## 6. Grammar / Parsing Shape

No grammar changes. Dependency aliases must conform to the existing import
path validation: each segment matches `[a-zA-Z_][a-zA-Z0-9_]*`. The parser
and `validImportPath()` are unchanged.

## 7. Lowering / Implementation Model

### Parser impact

None. No syntax changes.

### AST / IR impact

Dependency packages and import edges use typed `PackageId` values so equal
logical paths from different origins remain distinct.

### Checker impact

None. Dependency packages are checked through the same pipeline.

### Codegen impact

Dependency declarations are lowered identically to local packages, using
origin-safe canonical symbols derived from `PackageId`.

### Compiler package loader impact

- `load_package_graph()` constructs a `DependencyIndex` from `yar.toml` and
  `yar.lock` and gives it to `PackageLoader`.
- `DependencyIndex::load()` reconciles the complete lock graph, then builds
  source records and direct alias bindings for each owner origin without
  requiring every cache entry to exist.
- `DependencyIndex::resolve()` applies the importing origin's lookup scope and
  verifies a selected locked cache tree and manifest edges before exposing it.
- `PackageLoader::load_package()` records typed import targets, rejects
  duplicate final qualifiers, and keeps stdlib imports inside the stdlib origin.

### Runtime impact

None.

### Dependency implementation

- `crates/yar-compiler/src/manifest.rs` — parse and write `yar.toml`
- `crates/yar-compiler/src/manifest.rs` — parse and write `yar.lock`
- `crates/yar-compiler/src/manifest.rs` — git clone to cache, SHA-256 hash
  computation, pre-publication verification, transitive resolution, and
  conflict detection
- `crates/yar-compiler/src/lock_graph.rs` — graph reconciliation, selected
  manifest-edge verification, and targeted-update merge/prune behavior
- `crates/yar-compiler/src/package.rs` — origin-scoped source and alias lookup
  with selected locked-cache verification

### External dependency

- `toml` for TOML parsing.

## 8. Interactions

### Errors

No interaction. Dependency packages use the same error model.

### Structs, enums, interfaces, generics

No interaction. Exported types from dependencies are resolved through the
existing cross-package reference system.

### Import cycles

Cycle checks operate on `PackageId`, so equal logical paths from different
origins remain distinct and cycles through dependencies are detected normally.

### Stdlib

For non-stdlib importers, directly declared dependencies shadow stdlib packages
with the same qualifier. A locked dependency whose cache is missing or corrupt
fails before stdlib fallback. Stdlib's own imports never consult external
aliases.

### Testing

`yar test` works with dependencies. Test files in dependency packages are
excluded (only the root project's test files are included).

### Future modules/imports

The owner-scoped binding model is forward-compatible with richer module
conventions. Source-level import aliases (if added later) would compose with
dependency aliases.

## 9. Alternatives Considered

### URL-based import paths (Go-style)

Import paths like `import "github.com/user/yar-http/router"` encode the git
URL directly. Rejected because Yar's import path validation only accepts
`[a-zA-Z_][a-zA-Z0-9_]*` segments, which excludes dots and hyphens. Using
URLs would require parser changes and make import statements longer.

### Semver range resolution

Allow version ranges like `>=1.0.0, <2.0.0` with automatic resolution.
Rejected for complexity. Range resolution requires a SAT solver or MVS
algorithm. Exact pinning is simpler, deterministic without a solver, and
appropriate for an early-stage language. MVS can be added later as a
compatible extension.

### Flat lock entries without dependency edges

Rejected because a flat lock cannot distinguish a valid transitive package
from an unreachable extra, prove missing children, or update one dependency
without retaining stale descendants. Explicit child edges make the committed
graph independently reconcilable without opening unused caches.

### Global reachable-alias visibility

Expose every alias in the validated reachable lock graph to every package.
Rejected because reachability proves lock membership, not ownership or source
intent. Importers see only aliases declared by their source origin.

### Owner-local alias reuse in lock v1

Allow different owners to bind the same alias to different sources while still
writing lock v1. Deferred because lock v1 identifies package nodes through one
global alias namespace. The loader is owner-scoped, but representing distinct
targets for one alias requires lock v2.

### Root dependency overrides

Allow the root manifest to replace a transitive source/ref selection. Rejected
because the graph uses one exact global identity per alias. Conflicting uses of
an alias are errors regardless of where they are declared.

### Central registry

A package registry like crates.io or npm. Rejected because it requires
infrastructure, governance, and a critical mass of packages to be useful.
Git-based distribution works immediately with existing hosting platforms.

### Archive downloads instead of git clone

Download tarball archives from GitHub/GitLab instead of cloning. Rejected
because archive URLs are host-specific. Git clone works with any git host
including self-hosted ones. Archive optimization can be added later for known
hosts.

## 10. Complexity Cost

| Area                        | Cost                                                                  |
| --------------------------- | --------------------------------------------------------------------- |
| Language surface            | None — no syntax changes                                              |
| Parser complexity           | None                                                                  |
| Checker complexity          | None                                                                  |
| Lowering/codegen complexity | Low — typed package identity and origin-safe canonical symbols        |
| Compiler loader complexity  | Medium — source origins, owner bindings, and sealed stdlib lookup     |
| CLI complexity              | Medium — six new commands                                             |
| Dependency implementation   | Medium — manifest, lockfile, fetch, resolve, and package-index logic  |
| External dependency         | Low — one stable TOML parsing library                                 |
| Runtime complexity          | None                                                                  |
| Diagnostics complexity      | Medium — lock errors, undeclared aliases, and ambiguous qualifiers    |
| Test burden                 | Medium — unit tests for deps package, integration test for local deps |
| Documentation burden        | Medium — new domain doc, updates to summary, practices, YAR.md        |

## 11. Why Now?

Code reuse across projects is a prerequisite for building a package ecosystem.
Without dependency management, every Yar project is an island. This feature
enables library authors to publish and consumers to depend on shared code,
which is necessary before the language can grow beyond single-project use.

## 12. Open Questions

None remaining. All design decisions are resolved in the implementation.

## 13. Decision

Accepted. The alias-based, git-backed, exact-pinning design fits Yar's
preference for explicitness and simplicity. The versioned lock graph makes the
entire resolved closure explicit while preserving lazy cache access. Import
visibility is owner-scoped and package identity is origin-aware; lock v1 keeps
global alias/source uniqueness until a future lock v2 can encode true
owner-local alias reuse.

## 14. Implementation Checklist

- [x] `crates/yar-compiler/src/manifest.rs` — yar.toml parsing
- [x] `crates/yar-compiler/src/manifest.rs` — yar.lock parsing and writing
- [x] `crates/yar-compiler/src/manifest.rs` — git clone, caching, hashing
- [x] `crates/yar-compiler/src/manifest.rs` — transitive resolution, conflict
      detection
- [x] `crates/yar-compiler/src/lock_graph.rs` — exact graph reconciliation,
      selected-manifest verification, and targeted-update merging
- [x] `crates/yar-compiler/src/package.rs` — origin-scoped source/alias index
      and compiler integration
- [x] origin-scoped package identity, direct owner bindings, sealed stdlib
      imports, and origin-safe lowering
- [x] CLI commands in `crates/yar-cli/src/main.rs`
- [x] unit tests in `crates/yar-compiler`
- [x] integration test with local path dependency fixture
- [x] `docs/context/` updates
- [x] `docs/YAR.md` update
- [x] `decisions.md` update
