# Summary

## What

`yar` is a single-project Go compiler CLI for a small source language. It reads
an entry `.yar` file or package directory, resolves a package graph rooted
there, lowers that graph into one checked program, emits textual LLVM IR, and
can invoke `clang` with an embedded runtime to produce or run a native
executable.

## Architecture

- `cmd/yar` exposes the `check`, `emit-ir`, `build`, and `run` commands.
- `internal/token` defines the token set and source positions shared across stages.
- `internal/diag` defines source-positioned diagnostics.
- `internal/ast` defines the file AST plus the `Package` and `PackageGraph` structures used during package loading and lowering.
- `internal/compiler` resolves entry paths, loads local packages, falls back to embedded stdlib packages when local imports are missing, validates the import graph, rewrites package-local and imported symbols to canonical names, monomorphizes explicit generic instantiations, and orchestrates checking, IR generation, native linking, and execution.
- `internal/lexer` tokenizes source text into a token stream with lexical diagnostics.
- `internal/parser` builds file ASTs, including imports, generic type parameters and explicit type arguments, structs, interfaces, enums, methods, function literals and function types, loops, `match`, aggregate literals, pointers, and error-handling sugar.
- `internal/checker` owns semantic validation, scope tracking, struct, interface, and enum metadata, function, method, interface-method, and closure signatures, lexical capture analysis, builtin and host-intrinsic signatures, integer literal coercion, and program-wide error-code assignment.
- `internal/codegen` lowers the checked AST into textual LLVM IR, expanding concrete method calls into ordinary function calls with an explicit receiver argument, lowering interface values to boxed-data-plus-method-table pairs with dynamic dispatch, lowering function values to code-pointer-plus-environment closures, expanding error sugar, enum `match`, short-circuit boolean logic, aggregate values, loops, host-backed stdlib calls, the generated native `main` wrapper, and the shared runtime allocation and GC helpers.
- `internal/runtime` embeds the C runtime source that provides builtin I/O, panic behavior, string operations, map helpers, host filesystem and process calls, environment lookup, stderr output, argv capture, and the runtime-managed allocation plus conservative garbage-collection boundary used during linking.
- `internal/stdlib` embeds the standard library written in Yar (`strings`, `utf8`, `conv`, `sort`, `path`, `fs`, `process`, `env`, and `stdio`) and provides lookup functions for the package loader.

## Core Flow

- `check` resolves an entry file or package directory, runs `compiler.CompilePath`, and prints formatted diagnostics to stderr.
- `emit-ir` runs the same package loading, lowering, checking, and code-generation pipeline and writes LLVM IR to stdout.
- `build` compiles the entry package graph, writes IR and runtime C source into a temporary directory, and invokes `clang` to produce a native binary.
- `run` builds a temporary binary from the entry package graph and executes it with inherited stdin, stdout, and stderr.

## System State

- The repository contains one deployable unit: the `yar` CLI compiler.
- Programs are package graphs rooted at an entry `package main`, with one or more `.yar` files per package, explicit `import` declarations, package-qualified cross-package references, top-level `struct`, `interface`, `enum`, `fn`, and method declarations, and optional `pub` on exported structs, interfaces, enums, functions, and methods.
- Local imports resolve under the entry root directory. When a local import path is absent, the loader falls back to the embedded stdlib package of the same name. A local package shadows a stdlib package with the same import path.
- The implemented type system includes `bool`, `i32`, `i64`, `str`, `void`, `noreturn`, `error`, typed pointers, user-defined structs, user-defined interfaces, instantiated generic struct types, user-defined enums with optional payload cases, fixed arrays, slices, maps, and first-class function types.
- The language supports `:=`, `var`, assignment to locals, fields, array indices, slice indices, dereferences, and map elements, `if` / `else`, `for`, `break`, `continue`, exhaustive `match` over enum values, generic functions, generic structs with explicit type arguments, function literals with lexical capture-by-value, struct literals, enum constructors, array literals, slice literals, map literals, pointer address-of and dereference, `nil`, field access, concrete and interface method calls, function-value calls, indexing, slicing, unary `-`, unary `!`, short-circuit boolean `&&` / `||`, integer arithmetic including `%`, integer and boolean/string/pointer comparisons, string literals, explicit `error.Name` returns, `?` propagation sugar, `or |err| { ... }` local handling sugar, and direct propagation of matching errorable calls with `return`.
- String operations include `len(str)`, `str == str`, `str != str`, `str + str`, `s[i]`, and `s[i:j]`.
- Builtins are fixed in the compiler and runtime: `print(str)`, `print_int(i32)`, `panic(str)`, `len(array-or-slice-or-map-or-str)`, `append(slice, value)`, `has(map, key)`, `delete(map, key)`, and `keys(map)`. Three additional builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to the standard library and not available to user code.
- The embedded stdlib is imported like normal packages. `sort` provides in-place ascending helpers for `[]str`, `[]i32`, and `[]i64`; `path` provides pure path helpers; `fs` exposes host-backed text file and directory operations with explicit `error` behavior; `process` exposes the raw host argv plus child-process execution; `env` exposes environment lookup; and `stdio` provides stderr output.
- The executable boundary is native code produced by `clang`; the Go code does not interpret programs directly.

## Capabilities

- Parse and type-check source programs and surface source-positioned diagnostics.
- Emit textual LLVM IR without building a native executable.
- Build and run native executables backed by an embedded runtime C source.
- Propagate errors with direct `return` or postfix `?`.
- Handle errors locally with `or |err| { ... }`.
- Model closed variants with enums, payload-carrying enum cases, and exhaustive `match`.
- Support aggregate values and return types with structs, fixed arrays, slices, maps, and pointers.
- Reuse code through explicit generic structs and generic functions.
- Declare methods on named struct types with value or pointer receivers.
- Abstract over behavior with named interfaces, implicit concrete satisfaction, and dynamic interface calls.
- Define and return inline closures with explicit function types.
- Enumerate map keys through snapshot slices with `keys(map[K]V) []K`.
- Sort `[]str`, `[]i32`, and `[]i64` in place through the stdlib `sort` package.
- Support loops and branch-based control flow, including short-circuit boolean logic.
- Expose a runtime-managed allocation boundary with conservative garbage collection for slices, maps, pointers, and other heap-backed features.
- Reclaim unreachable heap-backed storage without adding user-visible lifetime syntax.
- Read and write text files, inspect directories, create temporary directories, and manipulate host paths from Yar programs.
- Read the host argument vector, look up environment variables, run child processes with captured or inherited stdio, and write diagnostics to stderr from Yar programs.
- Cross-compile to different OS/architecture targets using `YAR_OS` and `YAR_ARCH` environment variables without requiring knowledge of LLVM triples.

## Tech Stack

- Go 1.26 module with a single CLI entrypoint
- Custom lexer, parser, checker, and LLVM IR generator
- External `clang` invocation for compile and link, overridable via `CC`; cross-compilation targets specified via `YAR_OS` and `YAR_ARCH` environment variables
- Embedded C runtime source for builtin functions, host integration, and shared allocation / garbage-collection helpers, with `#ifdef _WIN32` conditionals for Windows platform support
- Embedded Yar standard library compiled through the same frontend as user code
- Go tests that validate compilation, executable output, panic behavior, unhandled errors, package imports, strings, maps, slices, pointers, enums, stdlib packages, host filesystem and process behavior, and toolchain/runtime boundaries
