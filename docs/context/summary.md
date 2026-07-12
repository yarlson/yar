# Summary

## What

`yar` is a Rust 2024 compiler CLI for a small source language. The shipped Rust CLI reads an entry `.yar` file or package directory and resolves, lowers, monomorphizes, and checks its package graph. `check` stops at that checked-program boundary; code-producing commands continue through textual LLVM IR generation and, when needed, invoke `clang` with the Rust runtime static library.

## Architecture

- `crates/yar-cli` exposes the `check`, `emit-ir`, `build`, `run`, `test`, `init`, `add`, `remove`, `fetch`, `lock`, and `update` commands.
- `crates/yar-compiler` is the Rust 2024 compiler crate. It defines token, diagnostic, AST, lexer, parser, package-graph loading, package-lowering, monomorphization, semantic checking, dependency manifests/locks, and LLVM emission. Its package loader identifies packages by source origin plus source-relative subpath, scopes local and declared-alias lookup to the importer origin, routes the reserved stdlib namespace, validates package names and import qualifiers, and checks import cycles; its lowerer enforces builtin shadowing and package export visibility and rewrites package graphs into one canonical program with origin-safe declaration names and imported function/type/enum-case references; its monomorphizer clones explicit generic struct and function instantiations into ordinary declarations and reports local generic call diagnostics with source-level names; its checker validates declarations and function bodies across the checked-in `testdata` corpus. Rust codegen emits clang-accepted LLVM for every checked-in `testdata/**/main.yar` entry program, including host-intrinsic stdlib calls, closures, interfaces, taskgroups, channels, pointers, enums, error handling, and the native main wrapper. `crates/yar-cli` provides the shipped Rust CLI for `check`, `emit-ir`, `build`, host `run`, host `test`, `init`, dependency manifest, lock, fetch, and update commands, and dependency-index package loading. Native paths validate target runtime bundles before linking: metadata fixes the target triple, runtime ABI, compiler compatibility, archive, and ordered system libraries. Explicit bundles use `YAR_RUNTIME_BUNDLE`; releases install one bundle under `runtimes/<target-triple>/`; workspace host builds reuse the same checked-in manifests. `crates/yar-process-control` is the shared boundary for typed tool-start errors, absolute subprocess deadlines, output draining, timed descendant containment, and Unix signal forwarding.
- `crates/yar-runtime` is the Rust 2024 runtime crate. It currently exports and tests C ABI-compatible low-level helpers, map and string-builder helpers, taskgroup and channel helpers, argv capture, environment lookup, child-process execution, filesystem helpers, and TCP networking helpers. String builders, streaming files, TCP listeners, and TCP connections use a validated process-local handle registry. The shipped Rust CLI links this runtime; the legacy embedded C runtime has been removed.
- `stdlib/packages` contains the standard library written in Yar (`strings`, `utf8`, `conv`, `sort`, `path`, `fs`, `io`, `process`, `env`, `stdio`, `net`, and `testing`) and is embedded by the Rust package loader.

## Core Flow

- `check` resolves an entry file or package directory, runs `yar_compiler::check_path`, and stops after semantic checking, printing formatted diagnostics to stderr on failure.
- `emit-ir` runs the same frontend, explicitly continues through LLVM generation, and writes the IR to stdout.
- `build` compiles the entry package graph, writes IR and a selected runtime input into a temporary directory, and invokes `clang` to produce a native binary under one configurable build deadline shared with any Cargo runtime build.
- `run` builds a temporary binary under the build deadline, then executes it without a default runtime deadline; values after `--` are forwarded unchanged and the program inherits stdin, stdout, and stderr.
- `test` includes `_test.yar` only for the selected entry package, diagnoses every malformed `test_*` declaration, generates a synthetic runner for valid tests, compiles it under the build deadline, and executes it under a separate configurable deadline.
- Project-aware commands accept a prefix-only `--manifest-path` override; otherwise compilation discovers from the entry directory and dependency commands from the invocation directory. The selected manifest directory anchors lock state, recovery, and relative dependency paths without changing the process working directory.
- `init` creates a `yar.toml` in the invocation directory or at an explicit target without discovering ancestors.
- `add` and `remove` resolve and serialize the complete target dependency state, then publish `yar.toml` with the target lock contents or absence as one recoverable transition.
- `fetch` downloads all dependencies in `yar.lock` to the global cache.
- `lock` regenerates `yar.lock` from `yar.toml` by resolving all git dependencies without rewriting the manifest.
- `update` re-resolves one or all dependencies and updates `yar.lock` without rewriting the manifest.

## System State

- The repository contains one deployable unit: the `yar` CLI compiler.
- Programs are package graphs rooted at an entry `package main`, with one or more `.yar` files per package, explicit `import` declarations, package-qualified cross-package references, top-level `struct`, `interface`, `enum`, `fn`, and method declarations, and optional `pub` on exported structs, interfaces, enums, functions, and methods.
- Package identity is a source origin plus a source-relative subpath. The reserved `std/...` namespace resolves only to embedded stdlib; other imports check same-origin packages and then aliases declared by that origin. Equal logical paths from different origins therefore do not share graph identity or internal symbols.
- The implemented type system includes `bool`, `i32`, `i64`, `str`, `void`, `noreturn`, `error`, first-class errorable value types (`!T` where produced by the language, such as taskgroup results), `chan[T]`, typed pointers, user-defined structs, user-defined interfaces, instantiated generic struct types, user-defined enums with optional payload cases, fixed arrays, slices, maps, and first-class function types.
- The language supports `:=`, `var`, assignment to locals, fields, array indices, slice indices, dereferences, and map elements, compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`) to locals, fields, array indices, slice indices, and dereferences with single target evaluation, `if` / `else`, `for { ... }`, `for cond { ... }`, `for init; cond; post { ... }`, `break`, `continue`, exhaustive `match` over enum values with optional `else` wildcard arm, generic functions, generic structs with explicit type arguments, function literals with lexical capture-by-value, `taskgroup []R { ... }` expressions, `spawn` statements within taskgroups, struct literals, enum constructors (keyed or positional for single-field cases), array literals, slice literals, map literals, pointer address-of and dereference with implicit pointer-to-struct dereference for field access, `nil`, field access, concrete and interface method calls, function-value calls, indexing, slicing with optional open-ended bounds (`s[i:]`, `s[:j]`), unary `-`, unary `!`, short-circuit boolean `&&` / `||`, integer arithmetic including `%`, integer, boolean, string, pointer, channel, and error comparisons, string literals, character literals (`'a'`), `error.Name` expressions and returns, `?` propagation sugar, `or |err| { ... }` local handling sugar, and direct propagation of matching errorable calls with `return`.
- Signed `i32` and `i64` addition, subtraction, multiplication, and negation wrap to the operand width. Division and remainder trap before execution for zero divisors and the signed overflow pair `MIN` and `-1`.
- String operations include `len(str)`, `str == str`, `str != str`, `str + str`, `s[i]`, `s[i:j]`, `s[i:]`, and `s[:j]`.
- Builtins are fixed in the compiler and runtime: `print(str)`, `panic(str)`, `len(array-or-slice-or-map-or-str)`, `append(slice, value)`, `has(map, key)`, `delete(map, key)`, `keys(map)`, `to_str(i32-or-i64-or-bool-or-str-or-error)`, string builder builtins `sb_new()`, `sb_write(handle, str)`, `sb_string(handle)`, and channel builtins `chan_new[T](capacity)`, `chan_send(ch, value)`, `chan_recv(ch)`, and `chan_close(ch)`. Three additional builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to the standard library and not available to user code.
- The embedded stdlib is imported through compiler-owned paths such as `std/fs` and `std/testing`. `sort` provides in-place ascending helpers for `[]str`, `[]i32`, and `[]i64`; `path` provides pure path helpers; `fs` exposes host-backed text file and directory operations plus streaming file handles with explicit `error` behavior; `io` defines stream interfaces and chunked copy/read helpers; `process` exposes the raw host argv plus child-process execution; `env` exposes environment lookup; `stdio` provides stderr output; `net` provides TCP networking primitives (listen, accept, connect, read, write, close, address info, timeouts, DNS resolution) with validated opaque handles, blocking I/O, and stream wrapper types; and `testing` provides test assertions using generic functions with `to_str`-based failure messages. No HTTP server package is embedded.
- The executable boundary is native code produced by `clang`; the compiler does not interpret programs directly.

## Capabilities

- Parse and type-check source programs and surface source-positioned diagnostics.
- Emit textual LLVM IR without building a native executable.
- Build and run native executables backed by the Rust runtime static library.
- Propagate errors with direct `return` or postfix `?`.
- Handle errors locally with `or |err| { ... }`.
- Run share-safe structured concurrent tasks with `taskgroup` and communicate through bounded typed channels.
- Model closed variants with enums, payload-carrying enum cases, and exhaustive `match`.
- Support aggregate values and return types with structs, fixed arrays, slices, maps, and pointers.
- Reuse code through explicit generic structs and generic functions.
- Declare methods on named struct types with value or pointer receivers.
- Abstract over behavior with named interfaces, implicit concrete satisfaction, and dynamic interface calls.
- Define and return inline closures with explicit function types.
- Enumerate map keys through snapshot slices with `keys(map[K]V) []K`.
- Sort `[]str`, `[]i32`, and `[]i64` in place through the stdlib `sort` package.
- Support loops and branch-based control flow, including short-circuit boolean logic.
- Expose one runtime-managed allocation boundary for slices, maps, pointers, and other heap-backed features.
- Reclaim unreachable managed heap storage with a conservative non-moving collector; no user-visible lifetime or deallocation syntax exists.
- Read and write text files, inspect directories, create temporary directories, and manipulate host paths from Yar programs.
- Stream file and TCP connection data through shared `io.Reader`, `io.Writer`, and `io.Closer` interfaces.
- Read the host argument vector, look up environment variables, run child processes with captured or inherited stdio, and write diagnostics to stderr from Yar programs.
- Cross-compile to different OS/architecture targets using `YAR_OS` and `YAR_ARCH` environment variables without requiring knowledge of LLVM triples.
- Discover and run test functions from `_test.yar` files using `yar test`, with generic assertion helpers from the `testing` stdlib package.
- Convert primitive values to their string representation with `to_str`.
- Compare error values with `==` and `!=`, and use `error.Name` as a general expression.
- Manage external dependencies through owner-scoped manifest or lock edges, recoverable project-metadata publication, git fetching, and commit-keyed caches verified against lock hashes. Lock v1 still requires each alias to identify one source/ref tuple across the graph.

## Tech Stack

- Rust 2024 workspace containing the shipped CLI, compiler rewrite, subprocess-control boundary, and runtime crates
- Custom lexer, parser, checker, and LLVM IR generator
- External `clang` invocation for compile and link, overridable via `CC`; cross-compilation targets specified via `YAR_OS` and `YAR_ARCH` environment variables
- Rust runtime static library for native linking
- Embedded Yar standard library compiled through the same frontend as user code
- Rust tests that validate compiler slices and the runtime crate's exported ABI helpers, plus Rust CLI verifier scripts that native-build and run checked-in `testdata/**/main.yar` fixtures
- GitHub Actions CI for formatting, linting, Linux/macOS workspace and fixture tests, Windows subprocess-control and native concurrency fixture tests, and release packaging dry runs
- GoReleaser-based GitHub Release CD for version tags
