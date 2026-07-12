# Source To Native

## Shared Frontend

- Path compilation loads a graph keyed by `PackageId(origin, subpath)` rather
  than by raw import text.
- The CLI resolves relative entry paths from its invocation directory. An
  explicit global manifest prefix selects the project root; otherwise path
  commands choose the nearest ancestor `yar.toml` from the entry directory. A
  manifestless entry uses its own directory as the root.
- The entry directory remains distinct from the project root and receives a
  project-relative package subpath. Explicit selection rejects an entry outside
  that root. Project selection does not change the process working directory.
- `std/<package>` imports resolve only to the embedded stdlib origin before any
  user-controlled lookup. Other imports resolve within the importing origin:
  own packages, aliases declared for that origin, then error.
- The loader rejects invalid paths, package-name mismatches, cycles, undeclared
  reachable aliases, and distinct imports with the same final-segment
  qualifier.
- Lowering follows each import's resolved `PackageId` and gives declarations
  origin-safe canonical names before monomorphization and checking. Those
  frontend stages produce a `CheckedProgram`; LLVM generation is an explicit
  downstream step for code-producing commands.

## `check`

- Implemented by the Rust CLI under `crates/yar-cli` through the Rust package
  load/lower/monomorphize/check pipeline.
- Resolves the named entry file or package directory on disk.
- When a root manifest is selected, builds origin-scoped dependency bindings
  from it, path-dependency manifests, and reconciled lock child edges.
- Runs package loading, lowering, monomorphization, and semantic checking
  through `yar_compiler::check_path`, then stops at `CheckedProgram`.
- Prints formatted diagnostics to stderr and exits non-zero when parsing or
  checking fails.
- Does not invoke LLVM generation or produce an executable artifact.

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
  through the CLI target resolver, falling back to the host platform when
  neither is set.
- Runs the same checked-program frontend as `check`, then explicitly emits LLVM
  with the resolved target triple.
- When a target is resolved, the generated LLVM IR includes a `target triple`
  directive.
- Writes the generated LLVM IR text to stdout on success.
- Stops before any `clang` invocation.

## `build`

- Implemented by the Rust CLI and links the Rust runtime static library
  directly.
- Accepts one entry file or package directory and an optional `-o` output path.
  The default output path is `a.out` on Unix and `a.exe` on Windows or when
  `YAR_OS=windows` is set. Relative output paths remain relative to the
  invocation directory.
- Resolves the build target from `YAR_OS` and `YAR_ARCH` environment variables.
  If neither is set, the host platform is used. If both are set, the compiler
  maps the pair to an LLVM target triple and passes `--target=<triple>` to
  `clang`. Supported targets: `darwin/amd64`, `darwin/arm64`, `linux/amd64`,
  `linux/arm64`, `windows/amd64`. The Windows target uses the GNU triple
  `x86_64-pc-windows-gnu`, matching the Rust release runtime archive.
- Runs `yar_compiler::compile_path`, which composes the checked-program frontend
  with LLVM generation, and aborts on diagnostics or code-generation failure.
- Emits `target triple` in the generated LLVM IR when a target is resolved.
- Creates a temporary build directory.
- Writes generated IR to `main.ll`.
- Builds `crates/yar-runtime` with `cargo build -p yar-runtime --release` and
  links the generated IR with the Rust runtime static library. This host-build
  path requires Cargo.
- The Rust CLI always uses the Rust runtime through a validated target bundle.
  An explicit `YAR_RUNTIME_BUNDLE` directory takes precedence, then packaged
  `runtimes/<target-triple>/` discovery. Host source builds use the checked-in
  manifest for the Cargo-built workspace archive.
- Bundle manifests must match the selected target, bundle format, runtime ABI,
  and compiler compatibility epoch. They name one safe relative archive and an
  ordered list of validated system libraries. Cross builds require a matching
  explicit or installed bundle.
- Invokes `clang -Wno-override-module [--target=<triple>] main.ll <archive> -o <output> <bundle-libraries>`.
- Applies one absolute `YAR_BUILD_TIMEOUT_SECS` deadline (30 seconds by default)
  across any Cargo runtime build and the clang invocation. Timed tools and their
  ordinary descendants are terminated before temporary build state is removed.
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
- Keeps the invocation directory as the program's working directory.
- Accepts `-- <argument>...` after the entry path and forwards every value after
  the delimiter unchanged. The temporary executable path remains argv element
  zero.
- Inherits stdin, stdout, and stderr from the calling process.
- Applies `YAR_BUILD_TIMEOUT_SECS` to native-build subprocesses but no default
  deadline to the user program. A numeric program exit status becomes the
  `yar run` exit status.
- Removes the temporary build directory and binary after execution.

## `test`

- Implemented by the Rust CLI through the Rust compiler/runtime path.
- Resolves the entry file or package directory.
- Rejects cross-compilation targets; `YAR_OS`/`YAR_ARCH` must match the host
  platform or be unset.
- Loads `_test.yar` files only for the selected entry package; imported package
  and dependency test files stay excluded.
- Validates every entry test-file `test_*` declaration and stops with
  source-positioned diagnostics if any candidate is malformed.
- Generates a synthetic test runner that replaces the user `main()`, creates a
  `testing.T` instance for each discovered test, calls each test function, and
  reports PASS/FAIL results to stdout.
- Compiles and executes the test binary through the same `clang` pipeline as
  `run`; the Rust CLI links the Rust runtime static library through the same
  archive lookup used by `build`.
- Executes the generated test binary under `YAR_TEST_TIMEOUT_SECS` (30 seconds
  by default), separately from the native build deadline.
- Keeps the invocation directory as the test program's working directory.
- Exit code is `0` when all tests pass, `1` when any test fails.
- Test files are excluded from `check`, `build`, `emit-ir`, and `run` commands.
