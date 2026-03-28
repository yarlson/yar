# Summary

## What

`yar` is a single-project Go compiler CLI for a small source language. It reads
one `.yar` source file, parses and type-checks it, emits LLVM IR text, and can
invoke `clang` with an embedded runtime to produce or run a native executable.

## Architecture

- `cmd/yar` exposes the `check`, `emit-ir`, `build`, and `run` commands.
- `internal/compiler` orchestrates parse, semantic check, IR generation, external linking, and process execution.
- `internal/parser` and `internal/lexer` turn source text into an AST and diagnostics.
- `internal/checker` owns semantic validation, local scope tracking, user-defined struct metadata, type inference for integer literals, builtin signatures, and error-code assignment.
- `internal/codegen` lowers the checked AST into textual LLVM IR, including explicit branches for error sugar, aggregate values, loops, the generated native `main` wrapper, and declarations for the shared runtime allocation helpers.
- `internal/runtime` embeds the small C runtime source that provides builtin I/O, panic behavior, and the shared allocation/trap boundary during linking.

## Core Flow

- `check` reads a source file, runs `compiler.Compile`, and prints formatted diagnostics to stderr.
- `emit-ir` runs the same frontend pipeline and writes the generated LLVM IR text to stdout.
- `build` compiles the source, writes IR and runtime C code into a temporary directory, and invokes `clang` to produce a native binary.
- `run` builds into a temporary binary and executes it with inherited stdin, stdout, and stderr.

## System State

- The repository contains one deployable unit: the `yar` CLI compiler.
- Programs are single-file `package main` sources with top-level `struct` and `fn` declarations.
- The implemented type system includes `bool`, `i32`, `i64`, `str`, `void`, `noreturn`, `error`, user-defined structs, and fixed arrays.
- The language supports `:=`, `var`, assignment to locals/fields/indices, `if` / `else`, `for`, `break`, `continue`, struct literals, array literals, field access, indexing, unary `-`, unary `!`, short-circuit boolean `&&` / `||`, integer arithmetic including `%`, integer and boolean comparisons, string literals, explicit `error.Name` returns, `?` propagation sugar, `or |err| { ... }` local handling sugar, and direct propagation of matching errorable calls with `return`.
- Builtins are fixed in the compiler and runtime: `print(str)`, `print_int(i32)`, `panic(str)`, and `len(array)`.
- The executable boundary is native code produced by `clang`; the Go code does not interpret programs directly.

## Capabilities

- Parse and type-check source programs and surface source-positioned diagnostics.
- Emit textual LLVM IR without building a native executable.
- Build and run native executables backed by an embedded runtime C source.
- Propagate errors with direct `return` or postfix `?`.
- Handle errors locally with `or |err| { ... }`.
- Support aggregate values and return types with structs and fixed arrays.
- Support loops and branch-based control flow for small real programs, including short-circuit boolean logic.
- Expose a runtime-managed allocation boundary internally for future heap-backed features.

## Tech Stack

- Go module with a single CLI entrypoint
- Textual LLVM IR generation
- External `clang` invocation for compile and link
- Embedded C runtime source for builtin functions and shared allocation helpers
- Go tests that validate compilation, executable output, panic behavior, unhandled errors, `i64` programs, and the v0.2 control-flow and aggregate surface
