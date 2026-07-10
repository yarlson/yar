# Summary

## What

`yar` is a Rust 2024 compiler CLI for a small source language. The shipped
Rust CLI reads an entry `.yar` file or package
directory, resolves a package graph rooted there, lowers that graph into one
checked program, emits textual LLVM IR, and invokes `clang` with the Rust
runtime static library to produce or run a native executable.

## Architecture

- `crates/yar-cli` exposes the `check`, `emit-ir`, `build`, `run`, `test`, `init`, `add`, `remove`, `fetch`, `lock`, and `update` commands.
- `crates/yar-compiler` is the Rust 2024 compiler crate. It defines token, diagnostic, AST, lexer, parser, package-graph loading, package-lowering, monomorphization, semantic checking, dependency manifests/locks, and LLVM emission. Its package loader resolves entry files/directories, local imports, dependency index entries, stdlib fallback, package names, and import cycles; its lowerer enforces builtin shadowing and package export visibility and rewrites package graphs into one canonical program with package-qualified declarations and imported function/type/enum-case references; its monomorphizer clones explicit generic struct and function instantiations into ordinary declarations and reports local generic call diagnostics with source-level names; its checker validates declarations and function bodies across the checked-in `testdata` corpus. Rust codegen emits clang-accepted LLVM for every checked-in `testdata/**/main.yar` entry program, including host-intrinsic stdlib calls, closures, interfaces, taskgroups, channels, pointers, enums, error handling, and the native main wrapper. `crates/yar-cli` provides the shipped Rust CLI for `check`, `emit-ir`, `build`, host `run`, host `test`, `init`, dependency manifest, lock, fetch, and update commands, and dependency-index package loading; its native build path supports cross targets when `YAR_RUNTIME_ARCHIVE` points at a target runtime archive, while sibling executable and workspace runtime archive fallbacks are host-only; release artifacts package the Rust CLI with a sibling runtime archive.
- `crates/yar-runtime` is the Rust 2024 runtime crate. It currently exports and tests C ABI-compatible low-level helpers, map and string-builder helpers, taskgroup and channel helpers, argv capture, environment lookup, child-process execution, filesystem helpers, and TCP networking helpers. The shipped Rust CLI links this runtime; the legacy embedded C runtime has been removed.
- `stdlib/packages` contains the standard library written in Yar (`strings`, `utf8`, `conv`, `sort`, `path`, `fs`, `io`, `process`, `env`, `stdio`, `net`, `http`, and `testing`) and is embedded by the Rust package loader.

## Core Flow

- `check` resolves an entry file or package directory, runs `compiler.CompilePath`, and prints formatted diagnostics to stderr.
- `emit-ir` runs the same package loading, lowering, checking, and code-generation pipeline and writes LLVM IR to stdout.
- `build` compiles the entry package graph, writes IR and a selected runtime input into a temporary directory, and invokes `clang` to produce a native binary.
- `run` builds a temporary binary from the entry package graph and executes it with inherited stdin, stdout, and stderr.
- `test` loads a package with `_test.yar` files included, discovers `test_*` functions, generates a synthetic test runner, compiles and executes the test binary, and reports pass/fail results.
- `init` creates a `yar.toml` manifest in the current directory.
- `add` adds a dependency to `yar.toml` and updates `yar.lock`.
- `remove` removes a dependency from `yar.toml` and updates `yar.lock`.
- `fetch` downloads all dependencies in `yar.lock` to the global cache.
- `lock` regenerates `yar.lock` from `yar.toml` by resolving all git dependencies.
- `update` re-resolves one or all dependencies and updates `yar.lock`.

## System State

- The repository contains one deployable unit: the `yar` CLI compiler.
- Programs are package graphs rooted at an entry `package main`, with one or more `.yar` files per package, explicit `import` declarations, package-qualified cross-package references, top-level `struct`, `interface`, `enum`, `fn`, and method declarations, and optional `pub` on exported structs, interfaces, enums, functions, and methods.
- Local imports resolve under the entry root directory. When a local import path is absent, the loader checks the dependency index built from `yar.toml` and `yar.lock`, then falls back to the embedded stdlib package of the same name. A local package shadows a dependency, and a dependency shadows a stdlib package with the same import path.
- The implemented type system includes `bool`, `i32`, `i64`, `str`, `void`, `noreturn`, `error`, first-class errorable value types (`!T` where produced by the language, such as taskgroup results), `chan[T]`, typed pointers, user-defined structs, user-defined interfaces, instantiated generic struct types, user-defined enums with optional payload cases, fixed arrays, slices, maps, and first-class function types.
- The language supports `:=`, `var`, assignment and compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`) to locals, fields, array indices, slice indices, dereferences, and map elements, `if` / `else`, `for { ... }`, `for cond { ... }`, `for init; cond; post { ... }`, `break`, `continue`, exhaustive `match` over enum values with optional `else` wildcard arm, generic functions, generic structs with explicit type arguments, function literals with lexical capture-by-value, `taskgroup []R { ... }` expressions, `spawn` statements within taskgroups, struct literals, enum constructors (keyed or positional for single-field cases), array literals, slice literals, map literals, pointer address-of and dereference with implicit pointer-to-struct dereference for field access, `nil`, field access, concrete and interface method calls, function-value calls, indexing, slicing with optional open-ended bounds (`s[i:]`, `s[:j]`), unary `-`, unary `!`, short-circuit boolean `&&` / `||`, integer arithmetic including `%`, integer, boolean, string, pointer, channel, and error comparisons, string literals, character literals (`'a'`), `error.Name` expressions and returns, `?` propagation sugar, `or |err| { ... }` local handling sugar, and direct propagation of matching errorable calls with `return`.
- String operations include `len(str)`, `str == str`, `str != str`, `str + str`, `s[i]`, `s[i:j]`, `s[i:]`, and `s[:j]`.
- Builtins are fixed in the compiler and runtime: `print(str)`, `panic(str)`, `len(array-or-slice-or-map-or-str)`, `append(slice, value)`, `has(map, key)`, `delete(map, key)`, `keys(map)`, `to_str(i32-or-i64-or-bool-or-str-or-error)`, string builder builtins `sb_new()`, `sb_write(handle, str)`, `sb_string(handle)`, and channel builtins `chan_new[T](capacity)`, `chan_send(ch, value)`, `chan_recv(ch)`, and `chan_close(ch)`. Three additional builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to the standard library and not available to user code.
- The embedded stdlib is imported like normal packages. `sort` provides in-place ascending helpers for `[]str`, `[]i32`, and `[]i64`; `path` provides pure path helpers; `fs` exposes host-backed text file and directory operations plus streaming file handles with explicit `error` behavior; `io` defines stream interfaces and chunked copy/read helpers; `process` exposes the raw host argv plus child-process execution; `env` exposes environment lookup; `stdio` provides stderr output; `net` provides TCP networking primitives (listen, accept, connect, read, write, close, address info, timeouts, DNS resolution) with opaque handles, blocking I/O, and stream wrapper types; `http` provides a minimal HTTP/1.1 server wrapper over `net`; and `testing` provides test assertions using generic functions with `to_str`-based failure messages.
- The executable boundary is native code produced by `clang`; the compiler does not interpret programs directly.

## Capabilities

- Parse and type-check source programs and surface source-positioned diagnostics.
- Emit textual LLVM IR without building a native executable.
- Build and run native executables backed by the Rust runtime static library.
- Propagate errors with direct `return` or postfix `?`.
- Handle errors locally with `or |err| { ... }`.
- Run structured concurrent tasks with `taskgroup` and communicate through
  bounded typed channels.
- Model closed variants with enums, payload-carrying enum cases, and exhaustive `match`.
- Support aggregate values and return types with structs, fixed arrays, slices, maps, and pointers.
- Reuse code through explicit generic structs and generic functions.
- Declare methods on named struct types with value or pointer receivers.
- Abstract over behavior with named interfaces, implicit concrete satisfaction, and dynamic interface calls.
- Define and return inline closures with explicit function types.
- Enumerate map keys through snapshot slices with `keys(map[K]V) []K`.
- Sort `[]str`, `[]i32`, and `[]i64` in place through the stdlib `sort` package.
- Support loops and branch-based control flow, including short-circuit boolean logic.
- Expose one runtime-managed allocation boundary for slices, maps, pointers,
  and other heap-backed features.
- Retain heap allocations until process exit in the current Rust runtime; no
  user-visible lifetime or deallocation syntax exists.
- Read and write text files, inspect directories, create temporary directories, and manipulate host paths from Yar programs.
- Stream file and TCP connection data through shared `io.Reader`, `io.Writer`, and `io.Closer` interfaces.
- Read the host argument vector, look up environment variables, run child processes with captured or inherited stdio, and write diagnostics to stderr from Yar programs.
- Serve small HTTP/1.1 responses over TCP with one request per connection through the stdlib `http` package.
- Cross-compile to different OS/architecture targets using `YAR_OS` and `YAR_ARCH` environment variables without requiring knowledge of LLVM triples.
- Discover and run test functions from `_test.yar` files using `yar test`, with generic assertion helpers from the `testing` stdlib package.
- Convert primitive values to their string representation with `to_str`.
- Compare error values with `==` and `!=`, and use `error.Name` as a general expression.
- Manage external dependencies through `yar.toml` manifests and `yar.lock` lock files, with git-based fetching, content-addressed caching, and transitive dependency resolution.

## Tech Stack

- Rust 2024 workspace containing the shipped CLI, compiler rewrite, and runtime crates
- Custom lexer, parser, checker, and LLVM IR generator
- External `clang` invocation for compile and link, overridable via `CC`; cross-compilation targets specified via `YAR_OS` and `YAR_ARCH` environment variables
- Rust runtime static library for native linking
- Embedded Yar standard library compiled through the same frontend as user code
- Rust tests that validate compiler slices and the runtime crate's exported ABI helpers, plus Rust CLI verifier scripts that native-build and run checked-in `testdata/**/main.yar` fixtures
- GitHub Actions CI for formatting, linting, Linux/macOS tests, and release packaging
  dry runs
- GoReleaser-based GitHub Release CD for version tags
