# Proposal: Cross-Platform Build and Runtime

Status: accepted

## 1. Summary

Add cross-compilation support via `YAR_OS` and `YAR_ARCH` environment variables
and port the embedded C runtime to support Windows alongside POSIX platforms
using conditional compilation.

The implemented version provides:

- `YAR_OS` / `YAR_ARCH` environment variables for target platform selection
- target triple resolution for LLVM code generation
- supported targets: darwin/amd64, darwin/arm64, linux/amd64, linux/arm64,
  windows/amd64
- host platform detection as default target
- cross-compilation validation (prevents running cross-compiled binaries)
- conditional compilation in the C runtime for Win32 and POSIX APIs
- platform-aware executable naming (`.exe` suffix for Windows)

## 2. Motivation

The compiler initially targeted only the host platform. This prevented building
binaries for other operating systems or architectures, limiting the language's
usefulness for distributed development and deployment.

Cross-compilation support requires two things:

1. a mechanism to specify the target platform and resolve the correct LLVM
   target triple
2. a runtime that compiles correctly for the target platform's system APIs

Environment variables were chosen over CLI flags because they compose naturally
with existing build workflows, can be set once for a session, and follow the
convention established by Go's `GOOS`/`GOARCH`.

Windows support was the primary driver because it required porting all
host-backed runtime functions (filesystem, process, environment) from POSIX to
Win32 APIs.

## 3. User-Facing Examples

### Valid examples

```
# Build for the host platform (default)
$ yar build main.yar

# Cross-compile for Linux on AMD64
$ YAR_OS=linux YAR_ARCH=amd64 yar build main.yar

# Cross-compile for Windows
$ YAR_OS=windows YAR_ARCH=amd64 yar build main.yar
# produces main.exe

# Run on host platform
$ yar run main.yar

# Test on host platform
$ yar test ./pkg
```

### Invalid examples

```
# Only one variable set — error
$ YAR_OS=linux yar build main.yar
# error: set both YAR_OS and YAR_ARCH, or neither
```

```
# Unsupported target combination — error
$ YAR_OS=windows YAR_ARCH=arm64 yar build main.yar
# error: unsupported target windows/arm64
```

```
# Cannot run cross-compiled binaries
$ YAR_OS=linux YAR_ARCH=amd64 yar run main.yar
# error: cannot execute cross-compiled binary
```

## 4. Semantics

- when neither `YAR_OS` nor `YAR_ARCH` is set, the compiler targets the host
  platform detected via Go's `runtime.GOOS` and `runtime.GOARCH`
- when both are set, the compiler looks up the LLVM target triple from a fixed
  mapping
- setting only one variable is an error
- unsupported OS/architecture combinations are an error
- cross-compiled binaries cannot be executed by `yar run` or `yar test`
- the target triple is passed to LLVM IR generation and to `clang` for linking
- Windows targets produce executables with the `.exe` suffix
- the C runtime uses `#ifdef _WIN32` conditional compilation to select between
  Win32 and POSIX implementations of host-backed functions

### Target triple mapping

| OS      | Arch  | LLVM Triple               |
| ------- | ----- | ------------------------- |
| darwin  | amd64 | x86_64-apple-darwin       |
| darwin  | arm64 | aarch64-apple-darwin      |
| linux   | amd64 | x86_64-unknown-linux-gnu  |
| linux   | arm64 | aarch64-unknown-linux-gnu |
| windows | amd64 | x86_64-pc-windows-msvc    |

## 5. Type Rules

No source-level type rules. Cross-compilation is a build-system concern that
does not affect the type system or source language.

## 6. Grammar / Parsing Shape

No new syntax. Target selection is entirely through environment variables.

## 7. Lowering / Implementation Model

- parser impact: none
- AST / IR impact: none
- checker impact: none
- codegen impact: the LLVM IR `target triple` directive is set based on the
  resolved target; data layout may differ per target
- compiler orchestration impact: `ResolveTarget()` reads environment variables,
  validates the combination, and returns a `Target` struct containing the
  triple, OS, arch, and whether it is a cross-compilation; `IsCross()` is
  checked before execution
- runtime impact: high; all host-backed C functions have conditional
  implementations:
  - filesystem: `CreateDirectoryA`, `FindFirstFileA`/`FindNextFileA`,
    `GetFileAttributesA`, `DeleteFileA`, `RemoveDirectoryA`,
    `GetTempPathA` on Windows vs `mkdir`, `opendir`/`readdir`, `stat`,
    `unlink`, `rmdir`, `getenv("TMPDIR")` on POSIX
  - process: `_popen`/`_pclose` on Windows vs `popen`/`pclose` on POSIX
  - environment: `getenv` on both (compatible)
  - I/O: `fprintf(stderr, ...)` on both (compatible)

## 8. Interactions

- errors: no interaction with the error model
- structs: no interaction
- arrays: no interaction
- control flow: no interaction
- returns: no interaction
- builtins: no interaction
- testing: `yar test` checks `IsCross()` and refuses to execute cross-compiled
  test binaries
- future modules/imports: no interaction
- future richer type features: no interaction

## 9. Alternatives Considered

- CLI flags (`--os`, `--arch`) instead of environment variables
  - must be passed on every invocation
  - do not compose as naturally with shell workflows
  - Go's `GOOS`/`GOARCH` convention is well-established
- separate runtime source files per platform
  - simpler per-file but harder to keep in sync
  - conditional compilation keeps related logic adjacent
- target the host only and defer cross-compilation
  - limits the language's practical utility
  - Windows support requires the runtime port regardless

## 10. Complexity Cost

- language surface: none
- parser complexity: none
- checker complexity: none
- lowering/codegen complexity: low (target triple in IR)
- compiler orchestration complexity: moderate (target resolution, validation,
  cross-compilation detection)
- runtime complexity: high (conditional compilation for all host-backed
  functions across Win32 and POSIX)
- diagnostics complexity: low
- test burden: moderate
- documentation burden: moderate

## 11. Why Now?

The language had reached a point where host-backed stdlib packages (filesystem,
process, environment) were stable. Supporting multiple platforms was a natural
next step toward making the language practically useful. Windows was the highest
priority non-host platform because of its prevalence in development
environments.

## 12. Open Questions

- should `windows/arm64` be added as a supported target?
- should additional Linux variants (musl, etc.) be supported?
- should the runtime eventually use platform-specific optimizations beyond
  API compatibility?

## 13. Decision

Accepted and implemented. Cross-compilation uses `YAR_OS` and `YAR_ARCH`
environment variables with a fixed target triple mapping. The C runtime uses
conditional compilation to support both Win32 and POSIX APIs from a single
source file.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `docs/context` update
- [x] `decisions.md` update
