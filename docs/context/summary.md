# Summary

## What

`yar` is a single-project Go compiler CLI for a small source language. It reads one `.yar` source file, parses and type-checks it, emits LLVM IR text, and can invoke `clang` with an embedded runtime to produce or run a native executable.

## Architecture

- `cmd/yar` exposes the `check`, `emit-ir`, `build`, and `run` commands.
- `internal/compiler` orchestrates parse, semantic check, IR generation, external linking, and process execution.
- `internal/parser` and `internal/lexer` turn source text into an AST and diagnostics.
- `internal/checker` owns semantic validation, local scope tracking, type inference for integer literals, builtin signatures, and error-code assignment.
- `internal/codegen` lowers the checked AST into textual LLVM IR and synthesizes the native `main` wrapper.
- `internal/runtime` embeds the small C runtime source that provides builtin I/O and panic behavior during linking.

## Core Flow

- `check` reads a source file, runs `compiler.Compile`, and prints formatted diagnostics to stderr.
- `emit-ir` runs the same frontend pipeline and writes the generated LLVM IR text to stdout.
- `build` compiles the source, writes IR and runtime C code into a temporary directory, and invokes `clang` to produce a native binary.
- `run` builds into a temporary binary and executes it with inherited stdin, stdout, and stderr.

## System State

- The repository contains one deployable unit: the `yar` CLI compiler.
- Programs are single-file `package main` sources with top-level function declarations.
- The implemented type system includes `bool`, `i32`, `i64`, `str`, `void`, and `noreturn`.
- The language supports `let`, assignment, `if`, `return`, function calls, integer and boolean comparisons, string literals, explicit `error.Name` returns, and direct propagation of matching errorable calls with `return`.
- Builtins are fixed in the compiler and runtime: `print(str)`, `print_int(i32)`, and `panic(str)`.
- The executable boundary is native code produced by `clang`; the Go code does not interpret programs directly.

## Capabilities

- Parse and type-check source programs and surface source-positioned diagnostics.
- Emit textual LLVM IR without building a native executable.
- Build and run native executables backed by an embedded runtime C source.
- Propagate matching errorable returns explicitly with `return`.
- Support integer arithmetic and comparisons across `i32`, `i64`, and inferred integer literals.

## Tech Stack

- Go module with a single CLI entrypoint
- Textual LLVM IR generation
- External `clang` invocation for compile and link
- Embedded C runtime source for builtin functions
- Go tests that validate compilation, executable output, panic behavior, unhandled errors, and `i64` programs
