# Proposal: Dependency Management

Status: accepted

## 1. Summary

Add git-based dependency management to Yar via `yar.toml` manifest and
`yar.lock` lock file, with no central registry.

The implemented version provides:

- `yar.toml` manifest declaring project metadata and dependencies by alias
- `yar.lock` lock file pinning git dependencies to exact commit SHAs and
  content hashes
- alias-based import mapping (dependency alias becomes the top-level import
  path segment)
- git-based fetching with shallow clone to a global user cache
- SHA-256 content hashing for integrity verification
- transitive dependency resolution with conflict detection
- local path overrides for development workflows
- CLI commands: `init`, `add`, `remove`, `fetch`, `lock`, `update`
- compiler integration via a dependency index consulted between local and
  stdlib resolution

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
  hashes, and writes `yar.lock`.
- During compilation, the package loader builds a dependency index from
  `yar.toml` and `yar.lock`. When a local import path is not found, the loader
  checks the dependency index before falling back to stdlib.
- Resolution order: local → dependency → stdlib.
- Local packages shadow dependencies. Dependencies shadow stdlib.
- Transitive dependencies are discovered by reading `yar.toml` in each
  fetched dependency. Conflicts (same alias, different version) are errors
  unless the root manifest provides an explicit override.
- `path` dependencies are resolved directly from the filesystem and are not
  written to `yar.lock`.

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

None. Dependency packages load into the same `ast.Package` structure.

### Checker impact

None. Dependency packages are checked through the same pipeline.

### Codegen impact

None. Dependency package declarations are lowered identically to local
packages.

### Compiler package loader impact

- `packageLoader` gains a `depIndex *deps.Index` field.
- `loadPackageGraph()` calls `loadDepIndex(rootDir)` to build the index from
  `yar.toml` and `yar.lock`.
- `loadPackage()` checks `l.depIndex.Resolve(importPath)` when a local
  directory is not found, before the stdlib fallback.

### Runtime impact

None.

### New package: `internal/deps/`

- `manifest.go` — parse and write `yar.toml`
- `lockfile.go` — parse and write `yar.lock`
- `fetch.go` — git clone to cache, SHA-256 hash computation, verification
- `resolve.go` — transitive resolution, conflict detection
- `index.go` — alias-to-path lookup for the package loader

### External dependency

- `github.com/BurntSushi/toml` for TOML parsing (first third-party Go
  dependency in the project).

## 8. Interactions

### Errors

No interaction. Dependency packages use the same error model.

### Structs, enums, interfaces, generics

No interaction. Exported types from dependencies are resolved through the
existing cross-package reference system.

### Import cycles

The existing `checkImportCycles()` works on the package graph regardless of
package source. Cycles through dependencies are detected identically.

### Stdlib

Dependencies shadow stdlib packages with the same alias name. This is
consistent with local packages already shadowing stdlib.

### Testing

`yar test` works with dependencies. Test files in dependency packages are
excluded (only the root project's test files are included).

### Future modules/imports

The alias-based system is forward-compatible with richer module conventions.
Import aliases (if added later) would compose naturally with dependency
aliases.

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
| Lowering/codegen complexity | None                                                                  |
| Compiler loader complexity  | Low — one new resolution step between local and stdlib                |
| CLI complexity              | Medium — six new commands                                             |
| New package complexity      | Medium — `internal/deps/` with five source files                      |
| External dependency         | Low — one stable TOML parsing library                                 |
| Runtime complexity          | None                                                                  |
| Diagnostics complexity      | Low — error messages for missing lock files, conflicts                |
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
preference for explicitness and simplicity. The implementation is minimal:
no parser changes, no type system changes, and one surgical integration point
in the package loader.

## 14. Implementation Checklist

- [x] `internal/deps/manifest.go` — yar.toml parsing
- [x] `internal/deps/lockfile.go` — yar.lock parsing and writing
- [x] `internal/deps/fetch.go` — git clone, caching, hashing
- [x] `internal/deps/resolve.go` — transitive resolution, conflict detection
- [x] `internal/deps/index.go` — alias-to-path index
- [x] compiler integration in `internal/compiler/packages.go`
- [x] CLI commands in `cmd/yar/main.go`
- [x] unit tests for `internal/deps/`
- [x] integration test with local path dependency fixture
- [x] `docs/context/` updates
- [x] `docs/YAR.md` update
- [x] `decisions.md` update
