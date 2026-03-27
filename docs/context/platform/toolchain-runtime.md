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

## Testing Boundary

- Compiler tests build real native executables and execute them.
- The test suite validates successful output, propagated unhandled errors, `panic`, `try`, and `i64` compilation through the same `clang` boundary used by the CLI.
