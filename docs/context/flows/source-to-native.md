# Source To Native

## `check`

- Resolves the named entry file or package directory on disk.
- Runs package loading, lowering, checking, and IR generation through
  `compiler.CompilePath`.
- Prints formatted diagnostics to stderr and exits non-zero when parsing or
  checking fails.
- Produces no IR or executable artifact.

## `emit-ir`

- Runs the same package loading, lowering, checking, and code-generation stages
  as `check`.
- Writes the generated LLVM IR text to stdout on success.
- Stops before any `clang` invocation.

## `build`

- Accepts one entry file or package directory and an optional `-o` output path.
  The default output path is `a.out` on Unix and `a.exe` on Windows or when
  `YAR_OS=windows` is set.
- Resolves the build target from `YAR_OS` and `YAR_ARCH` environment variables.
  If neither is set, the host platform is used. If both are set, the compiler
  maps the pair to an LLVM target triple and passes `--target=<triple>` to
  `clang`. Supported targets: `darwin/amd64`, `darwin/arm64`, `linux/amd64`,
  `linux/arm64`, `windows/amd64`.
- Re-runs `CompilePath` before native build and aborts on diagnostics.
- Emits `target triple` in the generated LLVM IR when a target is resolved.
- Creates a temporary build directory.
- Writes generated IR to `main.ll` and embedded runtime C code to `runtime.c`.
- Invokes `clang -Wno-override-module [--target=<triple>] main.ll runtime.c -o <output>`.
- Returns the produced native executable at the requested output path.
- Cross-compilation requires a `clang` installation that supports the requested
  target, including the appropriate sysroot and system libraries.

## `run`

- Resolves the entry file or package directory, checks it, and builds a
  temporary executable through `compiler.RunPath`.
- Rejects cross-compilation targets; `YAR_OS`/`YAR_ARCH` must match the host
  platform or be unset.
- Executes the produced binary as a subprocess.
- Does not forward user program arguments; the spawned program sees only the
  temporary executable path in its argv.
- Inherits stdin, stdout, and stderr from the calling process.
- Removes the temporary build directory and binary after execution.
