# Source To Native

## `check`

- Reads the named source file from disk.
- Runs the frontend pipeline through `compiler.Compile`.
- Prints formatted diagnostics to stderr and exits non-zero when parsing or checking fails.
- Produces no IR or executable artifact.

## `emit-ir`

- Runs the same parse and semantic stages as `check`.
- Writes the generated LLVM IR text to stdout on success.
- Stops before any `clang` invocation.

## `build`

- Accepts one source file and an optional `-o` output path. The default output path is `a.out`.
- Re-runs `Compile` before native build and aborts on diagnostics.
- Creates a temporary build directory.
- Writes generated IR to `main.ll` and embedded runtime C code to `runtime.c`.
- Invokes `clang -Wno-override-module main.ll runtime.c -o <output>`.
- Returns the produced native executable at the requested output path.

## `run`

- Reads the source file, checks it, and builds a temporary executable through `compiler.Run`.
- Executes the produced binary as a subprocess.
- Inherits stdin, stdout, and stderr from the calling process.
- Removes the temporary build directory and binary after execution.
