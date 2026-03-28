# Compiler Pipeline

## Responsibility Split

- `cmd/yar` is thin CLI wiring. It parses command names and basic arguments, reads the source file, formats diagnostics, and sets a timeout for `build` and `run`.
- `internal/compiler` is the orchestration boundary. It exposes:
  - `Compile(src)` for parse, semantic check, and IR generation
  - `Build(ctx, src, outputPath)` for IR emission plus `clang` linking
  - `Run(ctx, src)` for temporary build plus subprocess execution
- `internal/lexer` tokenizes source text, including control-flow, aggregate, and punctuation tokens, handles `//` comments and string escapes, and produces lexical diagnostics.
- `internal/parser` builds the AST for one source file, including top-level `struct` declarations, loops, aggregate literals, lvalue forms, and sugar nodes for `?` and `or |err| { ... }`, and appends parser diagnostics to lexer diagnostics.
- `internal/checker` validates package, struct, and function shape, tracks scopes, resolves builtin and user function signatures, resolves user-defined and array types, assigns expression types, validates loop and assignment-target rules, validates error-sugar legality, and records ordered error names.
- `internal/codegen` lowers the checked AST into LLVM IR, expanding error sugar into explicit checks, branches, and returns, lowering loops and aggregate values, generating the exported `main` wrapper around `yar.main`, and declaring the shared runtime allocation helpers used by future heap-backed features.
- `internal/runtime` exposes embedded runtime C source to the build step, including builtin I/O, panic behavior, and the shared allocation/trap boundary.

## Stage Contracts

- `Compile` returns a `Unit` only when parse and semantic checking succeed.
- Diagnostics stop code generation but do not count as Go errors.
- Code generation depends on `checker.Info` for expression types, function signatures, struct metadata, local types, and the program-wide error-code table.
- Front-end sugar is preserved through parsing and semantic analysis, then lowered during code generation rather than being represented as a runtime feature.
- Heap allocation support is modeled as runtime helper calls and trap behavior rather than as part of the explicit source-level `error` system.
- Native linking happens after IR generation by writing `main.ll` and `runtime.c` into a temporary directory and invoking `clang`.

## Generated Entry Boundary

- User code is emitted under `@yar.<function-name>`.
- Native process entry is a generated `@main` wrapper, not the user-defined function directly.
- Non-errorable `main` returns its `i32` result directly.
- Errorable `main` returns a generated result struct that the wrapper inspects to print an unhandled-error message or exit successfully.
