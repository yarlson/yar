# Compiler Pipeline

## Responsibility Split

- `cmd/yar` is thin CLI wiring. It parses command names and basic arguments, compiles an entry file or package directory, formats diagnostics, and sets a timeout for `build` and `run`.
- `internal/compiler` is the orchestration boundary. It exposes:
  - `Compile(src)` for in-memory single-file parse, semantic check, and IR generation used by focused tests
  - `CompilePath(path)` for package loading, lowering, semantic check, and IR generation from disk
  - `Build(ctx, src, outputPath)` / `Run(ctx, src)` for in-memory single-file build helpers
  - `BuildPath(ctx, path, outputPath)` / `RunPath(ctx, path)` for package builds from disk
- `internal/lexer` tokenizes source text, including control-flow, aggregate, pointer, and punctuation tokens, handles `//` comments and string escapes, and produces lexical diagnostics.
- `internal/parser` builds file ASTs, including top-level `struct` declarations, optional `pub` export markers, `import` declarations, loops, array and slice literals, pointer types, `nil`, index and slice postfix forms, generalized lvalue forms such as `(*ptr).field`, qualified call syntax, and sugar nodes for `?` and `or |err| { ... }`, and appends parser diagnostics to lexer diagnostics.
- `internal/compiler` lowers the loaded package graph into one combined checked program by rewriting package-local and imported symbols to canonical names after import and export validation.
- `internal/checker` validates struct and function shape, tracks scopes, resolves builtin and rewritten user function signatures, resolves user-defined, array, slice, and pointer types, assigns expression types, validates addressability and dereference rules, validates loop and assignment-target rules, validates slice indexing/slicing and `append`, validates error-sugar legality, and records ordered error names.
- `internal/codegen` lowers the checked AST into LLVM IR, expanding error sugar and short-circuit boolean operators into explicit checks, branches, and returns, lowering loops and aggregate values, lowering pointers to LLVM `ptr` values, lowering slices to runtime descriptors plus allocation/copy helpers, generating the exported `main` wrapper around `yar.main`, and declaring the shared runtime allocation helpers used by heap-backed features.
- `internal/runtime` exposes embedded runtime C source to the build step, including builtin I/O, panic behavior, slice bounds checks, and the shared allocation/trap boundary.

## Stage Contracts

- `Compile` returns a `Unit` only when parse and semantic checking succeed.
- Diagnostics stop code generation but do not count as Go errors.
- Code generation depends on `checker.Info` for expression types, function signatures, struct metadata, local types, and the program-wide error-code table.
- Front-end sugar is preserved through parsing and semantic analysis, then lowered during code generation rather than being represented as a runtime feature.
- Heap allocation support is modeled as runtime helper calls and trap behavior rather than as part of the explicit source-level `error` system.
- Pointer-taking of locals and parameters is currently implemented conservatively by storing local slots in runtime-managed storage so returned or retained addresses stay valid without a separate escape-analysis pass.
- Native linking happens after IR generation by writing `main.ll` and `runtime.c` into a temporary directory and invoking `clang`.

## Generated Entry Boundary

- User code is emitted under `@yar.<function-name>`.
- Native process entry is a generated `@main` wrapper, not the user-defined function directly.
- Non-errorable `main` returns its `i32` result directly.
- Errorable `main` returns a generated result struct that the wrapper inspects to print an unhandled-error message or exit successfully.
