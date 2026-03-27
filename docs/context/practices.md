# Practices

- The repository is organized as a single-project Go module with one user-facing runtime process: the `yar` CLI.
- Compilation is staged as lex/parse, semantic checking, LLVM IR generation, and optional native linking.
- Parse and semantic failures are returned as diagnostics; infrastructure failures such as file I/O or `clang` execution are returned as Go errors.
- Source programs must declare `package main`.
- A user-defined `main` function is required, and it must return `i32` or `!i32`.
- Local variables are introduced with `let`, scoped by blocks, and may be reassigned only after declaration.
- Errorable values cannot be bound, assigned, passed as arguments, used in conditions, or used in arithmetic.
- Errorable values may only leave a function through an explicit `return` from a function with the same errorable result type.
- Error names are collected across the program, sorted lexicographically, and then mapped to integer codes for the generated IR and native `main` wrapper.
- Builtins are compiler-owned contracts, not user-overridable functions.
- The runtime C source is embedded in the Go binary and materialized into a temporary file during native builds.
- The CLI places a timeout around `build` and `run` operations before invoking external processes.
