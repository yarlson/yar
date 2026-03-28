# Toolchain Runtime

## External Dependency

- Native builds depend on `clang` being available on `PATH`.
- The Go compiler emits textual LLVM IR and delegates machine-code generation and linking to `clang`.

## Embedded Runtime

- The runtime implementation lives in `internal/runtime/runtime_source.txt` and is embedded into the Go binary with `go:embed`.
- The build step materializes that source into a temporary `runtime.c` file rather than requiring cgo or a checked-in compiled artifact.

## Runtime Surface

- `yar_print(const char *data, long long len)` writes string data to stdout when the length is positive.
- `yar_print_int(int32_t value)` prints a signed 32-bit integer with `printf`.
- `yar_panic(const char *data, long long len)` writes the message to stderr, flushes stderr, and exits with status `1`.
- `yar_trap_oom(void)` terminates with `runtime failure: out of memory` on stderr and exit status `1`.
- `yar_alloc(long long size)` allocates runtime-managed storage and traps on invalid size or allocation failure.
- `yar_alloc_zeroed(long long size)` allocates zeroed runtime-managed storage and traps on invalid size or allocation failure.
- `yar_slice_index_check(long long index, long long len)` traps on out-of-range slice indexing.
- `yar_slice_range_check(long long start, long long end, long long len)` traps on invalid slice ranges.

## Allocation Boundary

- The compiler emits declarations for shared runtime allocation helpers and now uses them for user-visible pointer-supporting features.
- This establishes one allocation/trap boundary for future heap-backed features rather than separate per-feature runtime entry points.
- Slice literals, `append`, pointer composite literals, and local/parameter storage used by address-taking all reuse that same allocation boundary.
- Pointer composite literals lower by allocating storage for the pointed-to value and storing the literal into that storage.
- Allocation failure is currently treated as an unrecoverable runtime failure, not a YAR `error` value.

## Testing Boundary

- Compiler tests build real native executables and execute them.
- The test suite validates successful output, propagated unhandled errors, `panic`, `i64` compilation, slice behavior and traps, pointer behavior, enum definition and exhaustive `match`, v0.2 struct/array/loop programs, the `?` / `or |err| { ... }` error-sugar paths, multi-package imports, and the embedded allocation helper surface through the same `clang` boundary used by the CLI.
