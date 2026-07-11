# Source To Native

## `check`

- Implemented by the Rust CLI under `crates/yar-cli` through the Rust package
  load/lower/monomorphize/check pipeline.
- Resolves the named entry file or package directory on disk.
- If `yar.toml` exists in the root directory, builds a dependency index from
  the manifest and lock file for import resolution.
- Runs package loading, lowering, checking, and IR generation through
  `yar_compiler::compile_path`.
- Prints formatted diagnostics to stderr and exits non-zero when parsing or
  checking fails.
- Produces no IR or executable artifact.

## `emit-ir`

- Implemented by the Rust LLVM emitter, which has clang-accepted coverage for
  every current `testdata/**/main.yar` entry program, including
  scalar/control-flow, strings, fixed arrays, structs, slice operations, map
  operations, function literals, closure calls, concrete method calls,
  interface calls, taskgroups/spawn wrappers for named functions and immediate
  inline literals, channel builtin runtime calls, direct `fs`, `process`,
  `env`, `stdio`, and `net` host-intrinsic runtime calls, pointer operations,
  enum match lowering, and stdlib-internal builtins.
- Resolves the build target from `YAR_OS` and `YAR_ARCH` environment variables
  using `compiler.ResolveTarget`, falling back to the host platform when neither
  is set.
- Runs the same package loading, lowering, checking, and code-generation stages
  as `check`, passing the resolved target triple into code generation.
- When a target is resolved, the generated LLVM IR includes a `target triple`
  directive.
- Writes the generated LLVM IR text to stdout on success.
- Stops before any `clang` invocation.

## `build`

- Implemented by the Rust CLI and links the Rust runtime static library
  directly.
- Accepts one entry file or package directory and an optional `-o` output path.
  The default output path is `a.out` on Unix and `a.exe` on Windows or when
  `YAR_OS=windows` is set.
- Resolves the build target from `YAR_OS` and `YAR_ARCH` environment variables.
  If neither is set, the host platform is used. If both are set, the compiler
  maps the pair to an LLVM target triple and passes `--target=<triple>` to
  `clang`. Supported targets: `darwin/amd64`, `darwin/arm64`, `linux/amd64`,
  `linux/arm64`, `windows/amd64`. The Windows target uses the GNU triple
  `x86_64-pc-windows-gnu`, matching the Rust release runtime archive.
- Re-runs `CompilePath` before native build and aborts on diagnostics.
- Emits `target triple` in the generated LLVM IR when a target is resolved.
- Creates a temporary build directory.
- Writes generated IR to `main.ll`.
- Builds `crates/yar-runtime` with `cargo build -p yar-runtime --release` and
  links the generated IR with the Rust runtime static library. This host-build
  path requires Cargo.
- The Rust CLI always uses the Rust runtime and resolves its archive by first
  checking `YAR_RUNTIME_ARCHIVE`, then a `libyar_runtime.a`/`yar_runtime.lib`
  file next to the `yar` executable, and finally the workspace
  `target/release` archive after building `crates/yar-runtime`.
- Cross builds require `YAR_RUNTIME_ARCHIVE` to point at a runtime
  archive for the selected target; the sibling and workspace runtime archive
  fallbacks are host-only.
- Invokes `clang -Wno-override-module [--target=<triple>] main.ll <runtime-input> -o <output>`.
- Returns the produced native executable at the requested output path.
- Cross-compilation requires a `clang` installation that supports the requested
  target, including the appropriate sysroot and system libraries.

## `run`

- Implemented by the Rust CLI by building through the Rust compiler/runtime
  path into a temporary executable and executing it.
- Resolves the entry file or package directory, checks it, and builds a
  temporary executable through the Rust build path.
- Rejects cross-compilation targets; `YAR_OS`/`YAR_ARCH` must match the host
  platform or be unset.
- Executes the produced binary as a subprocess.
- Does not forward user program arguments; the spawned program sees only the
  temporary executable path in its argv.
- Inherits stdin, stdout, and stderr from the calling process.
- Removes the temporary build directory and binary after execution.

## `test`

- Implemented by the Rust CLI through the Rust compiler/runtime path.
- Resolves the entry file or package directory.
- Rejects cross-compilation targets; `YAR_OS`/`YAR_ARCH` must match the host
  platform or be unset.
- Loads the package graph with `_test.yar` files included (normally excluded).
- Scans test files for functions matching `fn test_*(t *testing.T) void`.
- Generates a synthetic test runner that replaces the user `main()`, creates a
  `testing.T` instance for each discovered test, calls each test function, and
  reports PASS/FAIL results to stdout.
- Compiles and executes the test binary through the same `clang` pipeline as
  `run`; the Rust CLI links the Rust runtime static library through the same
  archive lookup used by `build`.
- Exit code is `0` when all tests pass, `1` when any test fails.
- Test files are excluded from `check`, `build`, `emit-ir`, and `run` commands.
