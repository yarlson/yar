# Proposal: Host Filesystem and Path Utilities

Status: implemented

## 1. Summary

Add the minimum host-facing filesystem and path surface needed for compiler and
tooling programs.

The proposed standard-library packages are:

- `fs` for file and directory operations
- `path` for path manipulation

The initial API is intentionally small:

- `fs.read_file(path str) !str`
- `fs.write_file(path str, data str) !void`
- `fs.read_dir(path str) ![]fs.DirEntry`
- `fs.stat(path str) !fs.EntryKind`
- `fs.mkdir_all(path str) !void`
- `fs.remove_all(path str) !void`
- `fs.temp_dir(prefix str) !str`
- `path.clean(p str) str`
- `path.join(parts []str) str`
- `path.dir(p str) str`
- `path.base(p str) str`
- `path.ext(p str) str`

## 2. Motivation

The current language is now strong enough to express most in-memory frontend
logic:

- recursive ASTs through pointers
- symbol tables through maps
- dynamic collections through slices
- text work through string operations and UTF-8 helpers

The main remaining blocker for self-hosting is host interaction.

A self-hosted frontend needs to:

- discover whether an entry path names a file or directory
- read source files from disk
- list package directories and filter `.yar` files
- normalize and join host paths
- create output directories or temporary build directories
- write generated IR or other compiler outputs

Without an explicit host filesystem and path story, YAR can express parser and
checker logic but not the package-loading and artifact-writing boundary around
them.

## 3. User-Facing Examples

### Valid examples

```
import "fs"
import "path"

fn load_source(root str, pkg str, file str) !str {
    full := path.join([]str{root, pkg, file})
    return fs.read_file(full)
}
```

```
import "fs"
import "path"

fn collect_yar_files(dir str) ![]str {
    entries := fs.read_dir(dir)?
    files := []str{}
    for i := 0; i < len(entries); i = i + 1 {
        entry := entries[i]
        if entry.is_dir {
            continue
        }
        if path.ext(entry.name) == ".yar" {
            files = append(files, entry.name)
        }
    }
    return files
}
```

```
import "fs"
import "path"

fn emit_ir(out_dir str, ir str) !void {
    fs.mkdir_all(out_dir)?
    return fs.write_file(path.join([]str{out_dir, "main.ll"}), ir)
}
```

### Invalid examples

```
src := fs.read_file(1)
```

Invalid because filesystem functions operate on `str` paths.

```
dir := path.join("a", "b")
```

Invalid because the proposed `path.join` takes one `[]str` argument rather than
variadic arguments.

```
entries := fs.read_dir(path)?
name := entries.name
```

Invalid because `read_dir` returns a slice of `DirEntry`, not one entry.

## 4. Semantics

This proposal introduces a host-bound standard-library capability.

The important implementation constraint is to keep the host boundary as small as
possible.

The public `fs` and `path` packages should remain stdlib packages written in
Yar. Only the irreducible host-facing operations should require
compiler/runtime support. Deterministic path logic and higher-level filesystem
composition should stay in Yar source where practical.

### `fs`

`fs` exposes ordinary functions with explicit `error` behavior.

```
struct DirEntry {
    name str
    is_dir bool
}

enum EntryKind {
    File
    Directory
    Other
}
```

- `fs.read_file(path)` returns the full file contents as `str`
- `fs.write_file(path, data)` replaces or creates one file with the given text
- `fs.read_dir(path)` returns one snapshot slice of directory entries
- `fs.stat(path)` reports whether the path names a file, directory, or other
  host entry
- `fs.mkdir_all(path)` creates the directory path when missing
- `fs.remove_all(path)` recursively removes a file tree or file
- `fs.temp_dir(prefix)` creates one new temporary directory and returns its path

The first version is text-oriented. File contents are `str`, not `[]i32`,
`[]byte`, or opaque handles.

The public package surface does not imply that every exported `fs` function must
map directly to one runtime entry point.

The intended design is:

- keep low-level host-touching operations behind compiler-known intrinsics or
  runtime shims
- implement recursive or composed behavior in Yar where possible
- allow `fs.mkdir_all` and `fs.remove_all` to be ordinary Yar functions layered
  over smaller host primitives if that keeps the runtime smaller

All host failures are explicit errors. Expected error names in the initial
surface are:

- `error.NotFound`
- `error.PermissionDenied`
- `error.AlreadyExists`
- `error.InvalidPath`
- `error.IO`

The exact mapping from host errors to YAR error names is implementation-defined,
but the language/runtime contract must preserve these stable user-visible names.

### `path`

`path` is intentionally pure and non-errorable.

- `path.clean(p)` normalizes separators and removes redundant `.` / `..` forms
  when possible
- `path.join(parts)` joins host path segments using the current platform
  separator
- `path.dir(p)` returns the parent path
- `path.base(p)` returns the final path element
- `path.ext(p)` returns the suffix starting at the final `.`, or `""`

The intended implementation is that `path` lives entirely in Yar unless one
small platform-normalization hook proves necessary. The default design target is
to keep `path` logic out of the runtime.

This proposal keeps import-path semantics unchanged. Import strings remain
slash-separated logical package paths. `path` is for host filesystem paths, not
for reinterpreting import syntax.

## 5. Type Rules

- every `fs` function that accepts a path requires `str`
- `fs.read_file` returns `!str`
- `fs.write_file` returns `!void`
- `fs.read_dir` returns `![]fs.DirEntry`
- `fs.stat` returns `!fs.EntryKind`
- `fs.mkdir_all` returns `!void`
- `fs.remove_all` returns `!void`
- `fs.temp_dir` returns `!str`
- `path.clean`, `path.join`, `path.dir`, `path.base`, and `path.ext` all accept
  and return `str`
- raw errorable `fs` calls remain subject to the ordinary error rules: they must
  be returned, propagated with `?`, or handled with `or |err| { ... }`

## 6. Grammar / Parsing Shape

No new grammar is required.

This proposal is expressed entirely through standard-library packages and
ordinary package-qualified calls:

- `fs.read_file(path)`
- `fs.read_dir(dir)`
- `path.join(parts)`

## 7. Lowering / Implementation Model

- parser: no changes
- AST / IR: no new node kinds
- checker: no new syntax rules; register package-loaded function signatures in
  the same way as other stdlib APIs
- codegen: lower selected stdlib calls to a minimal set of compiler-known host
  intrinsics or runtime shims
- runtime: expose only the low-level host boundary that cannot be expressed
  cleanly in Yar or portable generated LLVM

The preferred split is:

- move all deterministic path logic into Yar
- move recursive or composed filesystem logic into Yar where possible
- keep only low-level host calls as intrinsics/runtime shims
- let codegen perform the lowering from public stdlib calls to those low-level
  shims

This means the runtime surface should be smaller than the public stdlib surface.
For example, it is acceptable for public `fs.mkdir_all` or `fs.remove_all` to be
implemented in Yar on top of lower-level host operations rather than mirrored as
1:1 runtime functions.

The most important design choice is that `fs` is still presented as standard
library, not as a new family of ad hoc builtins. The compiler may embed the
package source and treat selected declarations as host intrinsics during
lowering, similar in spirit to the existing runtime boundary for allocation and
string helpers.

This gives YAR a minimal host boundary without introducing a general-purpose
FFI.

## 8. Interactions

- errors: host failures fit the existing explicit error model naturally
- structs: `DirEntry` is an ordinary struct and should feel consistent with the
  rest of the language
- arrays: no special interaction
- control flow: host operations introduce no hidden control flow beyond explicit
  `!T` failure
- returns: filesystem results and errors return like any other values
- builtins: no new syntax-level builtin is required
- future modules/imports: package loading depends directly on `fs` and `path`
- future richer type features: later byte buffers or richer path types can layer
  on top of this text-first baseline

## 9. Alternatives Considered

### Keep host I/O outside the language entirely

Rejected because a self-hosted frontend still needs to load packages and emit
artifacts from Yar code, not from a permanently privileged Go wrapper.

### Add file I/O as builtins only

Rejected because these operations belong to a clear host-stdlib boundary rather
than to the global builtin namespace.

### Mirror the public `fs` / `path` API directly in the runtime

Rejected as the default design because it makes the runtime surface grow too
quickly and duplicates logic that can live in Yar or codegen. The public stdlib
API should be larger than the irreducible host boundary when that keeps the
runtime smaller and clearer.

### Add a much larger POSIX-like API immediately

Rejected because the self-hosting need is narrow: read source files, enumerate
packages, and write outputs. A large syscall-like surface would exceed the
current complexity budget.

## 10. Complexity Cost

- language surface: medium
- parser complexity: low
- checker complexity: low
- lowering/codegen complexity: high
- runtime complexity: medium
- diagnostics complexity: medium
- test burden: high
- documentation burden: high

## 11. Why Now?

This is the clearest remaining capability blocker between the current language
and a self-hosted frontend.

The in-memory language model is already far enough along. What is missing now is
the ability to connect frontend logic to the host filesystem and artifact
boundary.

## 12. Open Questions

- Should `fs.read_file` and `fs.write_file` stay text-only in the first version,
  or should a later proposal add binary buffers?
- Should `fs.read_dir` return entries in host order, sorted order, or
  unspecified order?
- Should `path` use platform-native separators in all returned strings, or
  preserve slash forms where possible?
- What is the smallest low-level host primitive set that still lets Yar
  implement `fs.mkdir_all` and `fs.remove_all` without pushing that logic back
  into the runtime?
- Should local packages be allowed to shadow host stdlib package names such as
  `fs` and `path`, or should some package names be reserved?

## 13. Decision

Proposed.

This should be evaluated as part of a self-hosting milestone rather than
accepted in isolation from the rest of the host-bound surface.

## 14. Implementation Checklist

- stdlib package API design
- runtime host I/O boundary
- lowering/codegen hooks for host calls
- Yar-level implementation plan for `path`
- Yar-level implementation plan for recursive/composed `fs` helpers
- diagnostics for host-failure error names
- integration tests for file loading and output writing
- package-loader migration plan
- `current-state.md` update
- `decisions.md` update
