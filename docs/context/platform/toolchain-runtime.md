# Toolchain Runtime

## External Dependency

- Native builds depend on `clang` being available on `PATH`.
- The Go compiler emits textual LLVM IR and delegates machine-code generation and linking to `clang`.
- The `CC` environment variable overrides the default compiler command; `findCC()` in `internal/compiler/cc.go` checks `CC` first, then falls back to `"clang"`.
- When the compiler is not found, the error message names the missing command and suggests installing clang or setting `CC`.
- On Windows, temporary executables produced by `Run` and `RunPath` use a `.exe` suffix so the OS can execute them.
- The default output name for `build` is `a.out` on Unix and `a.exe` on Windows.

## Embedded Runtime

- The runtime implementation lives in `internal/runtime/runtime_source.txt` and is embedded into the Go binary with `go:embed`.
- The build step materializes that source into a temporary `runtime.c` file rather than requiring cgo or a checked-in compiled artifact.

## Runtime Surface

### I/O and Control

- `yar_print(const char *data, long long len)` writes string data to stdout when the length is positive.
- `yar_print_int(int32_t value)` prints a signed 32-bit integer with `printf`.
- `yar_panic(const char *data, long long len)` writes the message to stderr, flushes stderr, and exits with status `1`.
- `yar_eprint(const char *data, long long len)` writes string data to stderr when the length is positive and flushes stderr.
- The generated native `main` wrapper now accepts `argc` / `argv`, forwards them to `yar_set_args(int32_t argc, char **argv)`, and then calls user `yar.main()`.

### Allocation

- `yar_trap_oom(void)` terminates with `runtime failure: out of memory` on stderr and exit status `1`.
- `yar_alloc(long long size)` allocates runtime-managed storage and traps on invalid size or allocation failure.
- `yar_alloc_zeroed(long long size)` allocates zeroed runtime-managed storage and traps on invalid size or allocation failure.

### Slice Runtime

- `yar_slice_index_check(long long index, long long len)` traps on out-of-range slice indexing.
- `yar_slice_range_check(long long start, long long end, long long len)` traps on invalid slice ranges.

### String Runtime

- `yar_str_equal(const char *a_ptr, long long a_len, const char *b_ptr, long long b_len)` compares two strings by length then bytes, returning 1 for equal, 0 otherwise.
- `yar_str_concat(const char *a_ptr, long long a_len, const char *b_ptr, long long b_len)` allocates and returns a new string containing the concatenation of both inputs.
- `yar_str_index_check(long long index, long long len)` traps on out-of-range string indexing.
- `yar_str_from_byte(int32_t value)` allocates a one-byte string from a byte value; traps if the value is outside the 0-255 range.

### Filesystem Runtime

- `yar_fs_read_file(yar_str path, yar_str *out)` reads a whole file into one runtime-managed string and returns a stable filesystem status code.
- `yar_fs_write_file(yar_str path, yar_str data)` writes one whole file and returns a stable filesystem status code.
- `yar_fs_read_dir(yar_str path, yar_slice *out)` returns a slice of `fs.DirEntry`-layout values (`name`, `is_dir`) and a stable filesystem status code.
- `yar_fs_stat(yar_str path, int32_t *kind_out)` classifies a path as file, directory, or other.
- `yar_fs_mkdir_all(yar_str path)` creates a directory tree.
- `yar_fs_remove_all(yar_str path)` recursively removes a file or directory tree.
- `yar_fs_temp_dir(yar_str prefix, yar_str *out)` creates one temporary directory under `TMPDIR` or `/tmp`.
- Runtime filesystem status codes map in codegen to stable YAR error names: `NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidPath`, and `IO`.
- The current implementation uses POSIX interfaces (`stat`, `opendir`, `mkdir`, `remove`, `mkstemp`) and normalizes paths through the `path` stdlib package rather than a platform-specific separator API.

### Process / Environment Runtime

- `yar_process_args(yar_slice *out)` copies the host argument vector into a runtime-managed `[]str`.
- `yar_process_run(const yar_slice *argv, yar_process_result *out)` launches one child process, captures stdout/stderr into runtime-managed strings, and returns a stable host-process status code.
- `yar_process_run_inherit(const yar_slice *argv, int32_t *exit_code_out)` launches one child with inherited stdin/stdout/stderr and returns a stable host-process status code.
- `yar_env_lookup(yar_str name, yar_str *out)` looks up one environment variable and returns a stable host-process status code.
- Host-process status codes map in codegen to stable YAR error names: `NotFound`, `PermissionDenied`, `InvalidArgument`, and `IO`.
- The current implementation uses POSIX interfaces (`fork`, `execvp`, `waitpid`, `mkstemp`, `getenv`) and currently captures child stdout/stderr through temporary files.

### Map Runtime

- `yar_map_new(int32_t key_kind, int32_t key_size, int32_t value_size)` allocates a new open-addressed hash map with initial capacity 8.
- `yar_map_set(void *map_ptr, const void *key, const void *value)` inserts or replaces an entry, growing the table at 75% load.
- `yar_map_get(void *map_ptr, const void *key, void *value_out)` looks up a key, copies the value into `value_out`, and returns 1 if found or 0 otherwise.
- `yar_map_has(void *map_ptr, const void *key)` returns 1 if the key exists, 0 otherwise.
- `yar_map_delete(void *map_ptr, const void *key)` removes the entry for a key and rehashes forward entries to preserve linear probing.
- `yar_map_len(void *map_ptr)` returns the current entry count.
- Key kinds are passed from codegen as integer constants: `bool` (0), `i32` (1), `i64` (2), `str` (3).
- Maps use FNV-1a hashing with linear probing and power-of-two capacity.

## Allocation Boundary

- The compiler emits declarations for shared runtime allocation helpers and now uses them for user-visible pointer-supporting features.
- This establishes one allocation/trap boundary for future heap-backed features rather than separate per-feature runtime entry points.
- Slice literals, `append`, pointer composite literals, map allocations, and local/parameter storage used by address-taking all reuse that same allocation boundary.
- Pointer composite literals lower by allocating storage for the pointed-to value and storing the literal into that storage.
- Map creation and growth allocate through `malloc`/`calloc` with OOM trapping.
- Allocation failure is treated as an unrecoverable runtime failure, not a YAR `error` value.

## Testing Boundary

- Compiler tests build real native executables and execute them.
- The test suite validates successful output, propagated unhandled errors, `panic`, `i64` compilation, slice behavior and traps, pointer behavior, enum definition and exhaustive `match`, map operations, v0.2 struct/array/loop programs, the `?` / `or |err| { ... }` error-sugar paths, multi-package imports, string operations (including indexing, slicing, and concatenation edge cases), stdlib imports, host filesystem/path behavior, host process/environment behavior, CC override behavior, internal builtin rejection, and the embedded allocation helper surface through the same `clang` boundary used by the CLI.
