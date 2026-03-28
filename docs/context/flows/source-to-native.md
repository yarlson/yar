# Source To Native

## `check`

- Resolves the named entry file or package directory on disk.
- Runs the package frontend pipeline through `compiler.CompilePath`.
- Prints formatted diagnostics to stderr and exits non-zero when parsing or checking fails.
- Produces no IR or executable artifact.

## `emit-ir`

- Runs the same package loading, parse, lowering, and semantic stages as `check`.
- Writes the generated LLVM IR text to stdout on success.
- Stops before any `clang` invocation.

## `build`

- Accepts one entry file or package directory and an optional `-o` output path. The default output path is `a.out`.
- Re-runs `CompilePath` before native build and aborts on diagnostics.
- Creates a temporary build directory.
- Writes generated IR to `main.ll` and embedded runtime C code to `runtime.c`.
- Invokes `clang -Wno-override-module main.ll runtime.c -o <output>`.
- Returns the produced native executable at the requested output path.

## `run`

- Resolves the entry file or package directory, checks it, and builds a temporary executable through `compiler.RunPath`.
- Executes the produced binary as a subprocess.
- Inherits stdin, stdout, and stderr from the calling process.
- Removes the temporary build directory and binary after execution.
