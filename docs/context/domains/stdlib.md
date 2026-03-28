# Standard Library

## Design

- The standard library is written in yar, not C or Go.
- Stdlib packages are embedded into the Go compiler binary using `go:embed` via the `internal/stdlib` package.
- Stdlib packages are imported with bare paths like any user package: `import "strings"`.
- Resolution order: local packages first, embedded stdlib second. A local directory with the same name shadows the stdlib package.
- Stdlib packages are parsed, type-checked, and compiled through the same pipeline as user code.
- Most stdlib functions are ordinary yar code. A small set of embedded `fs`, `process`, `env`, and `stdio` declarations are tagged as host intrinsics during checking/codegen and lower to runtime shims while keeping the user-facing API package-shaped.

## Infrastructure

- `internal/stdlib/stdlib.go` provides `Has`, `ReadDir`, and `ReadFile` for the package loader.
- `internal/stdlib/packages/<pkg>/<file>.yar` is the canonical location for stdlib source files.
- `internal/compiler/packages.go` calls `loadStdlibPackage` as a fallback when `os.ReadDir` fails with `os.ErrNotExist` for an import path.
- Both local and stdlib packages share the same `resolveImports` method for import resolution.
- Stdlib packages can import other stdlib packages.

## Packages

### `strings`

Practical string operations built on proposal 0006 primitives (`len(str)`, `s[i]`, `s[i:j]`, `==`, `+`).

Functions:

- `contains(s str, substr str) bool` — linear scan with slice compare
- `has_prefix(s str, prefix str) bool` — compare prefix slice
- `has_suffix(s str, suffix str) bool` — compare suffix slice
- `index(s str, substr str) i32` — byte offset or -1
- `count(s str, substr str) i32` — non-overlapping occurrences
- `repeat(s str, n i32) str` — concatenation loop
- `replace(s str, old str, new str, n i32) str` — find-and-replace, n < 0 means all
- `trim_left(s str, cutset str) str` — strip leading bytes in cutset
- `trim_right(s str, cutset str) str` — strip trailing bytes in cutset
- `join(parts []str, sep str) str` — join slice of strings

Internal helpers `contains_byte` and `parse_positive` are not exported.

Additional functions (proposal 0008):

- `from_byte(i32) str` — construct a single-byte string (wraps `chr` builtin)
- `parse_i64(str) !i64` — parse a base-10 signed integer; returns `error.InvalidInteger` or `error.IntegerOverflow`

### `utf8`

UTF-8 decoding and rune classification for lexers and diagnostic code.

Functions:

- `decode(s str, off i32) !i32` — decode the rune at byte offset `off`
- `width(s str, off i32) !i32` — byte width of the rune at byte offset `off`
- `is_letter(r i32) bool` — letter or underscore classification (ASCII plus common Unicode letter ranges)
- `is_digit(r i32) bool` — ASCII digit 0–9
- `is_space(r i32) bool` — Unicode whitespace codepoints

Errors: `error.InvalidUTF8` for malformed sequences, `error.OutOfRange` for invalid offsets.

### `conv`

Type conversion and integer-to-string functions.

Functions:

- `to_i64(n i32) i64` — widen i32 to i64 (wraps internal `i32_to_i64` builtin)
- `to_i32(n i64) i32` — truncate i64 to i32 (wraps internal `i64_to_i32` builtin)
- `byte_to_str(b i32) str` — one-byte string from byte value (wraps internal `chr` builtin)
- `itoa(n i32) str` — base-10 decimal string from i32
- `itoa64(n i64) str` — base-10 decimal string from i64

Depends on `strings.from_byte` for digit character construction.

### `path`

Pure path helpers for host-facing tooling code.

Functions:

- `clean(p str) str` — normalize `\` to `/`, collapse repeated separators, and simplify `.` / `..` segments
- `join(parts []str) str` — join path segments with `/` then clean the result
- `dir(p str) str` — parent path, or `.` when there is no separator
- `base(p str) str` — final path element
- `ext(p str) str` — suffix from the final `.`, or `""`

Current constraint: the implementation normalizes to forward slashes rather than emitting an OS-specific separator.

### `fs`

Host-backed text-oriented filesystem helpers.

Types:

- `DirEntry { name str, is_dir bool }`
- `EntryKind { File, Directory, Other }`

Functions:

- `read_file(path str) !str` — read a whole text file into one `str`
- `write_file(path str, data str) !void` — create or replace one text file
- `read_dir(path str) ![]DirEntry` — snapshot a directory entry list
- `stat(path str) !EntryKind` — classify one host entry
- `mkdir_all(path str) !void` — create a directory tree
- `remove_all(path str) !void` — recursively remove a file or directory tree; a missing path is treated as success
- `temp_dir(prefix str) !str` — create one new temporary directory and return its path

Errors:

- `error.NotFound`
- `error.PermissionDenied`
- `error.AlreadyExists`
- `error.InvalidPath`
- `error.IO`

### `process`

Host-backed process and argv helpers.

Types:

- `Result { exit_code i32, stdout str, stderr str }`

Functions:

- `args() []str` — return the host-provided argument vector
- `run(argv []str) !Result` — launch one child process, capture stdout/stderr, and return the child exit code plus captured output
- `run_inherit(argv []str) !i32` — launch one child process with inherited stdin/stdout/stderr and return the child exit code

Errors:

- `error.NotFound`
- `error.PermissionDenied`
- `error.InvalidArgument`
- `error.IO`

### `env`

Host-backed environment lookup.

Functions:

- `lookup(name str) !str` — return one environment variable value, or `error.NotFound` when absent

Additional current failure mode:

- `error.InvalidArgument` for names that cannot cross the host boundary (for example embedded NUL bytes)

### `stdio`

Host-backed stderr output.

Functions:

- `eprint(msg str) void` — write one string to stderr

## Constraints

- Stdlib packages have access to internal builtins (`chr`, `i32_to_i64`, `i64_to_i32`) that are not available to user code. The `conv` package exposes these as public wrappers. Other stdlib packages (e.g., `strings`) also call them directly.
- Performance is naive and correct. Concatenation-heavy functions like `repeat`, `replace`, `itoa`, and `itoa64` are O(n^2) for large inputs — acceptable for the current stage.
- Stdlib packages are not versioned separately from the compiler.
- The `fs` runtime boundary is currently POSIX-oriented (`stat`, `opendir`, `mkdir`, `remove`, `TMPDIR`) rather than a full cross-platform abstraction.
- The `process` runtime boundary is also POSIX-oriented in the first version (`fork`, `execvp`, `waitpid`, `mkstemp`) and currently captures child stdout/stderr through temporary files before copying them into runtime-managed strings.

## Adding a New Package

1. Create `internal/stdlib/packages/<name>/<name>.yar` declaring `package <name>`.
2. Mark exported functions with `pub`.
3. No Go code changes needed — the embed and loader handle discovery automatically.
4. Add integration tests in `internal/compiler/compiler_test.go` and a fixture in `testdata/`.
5. Document in `docs/YAR.md` under the Standard Library section.

## Testing

- `internal/stdlib/stdlib_test.go` covers embedding: `Has`, `ReadDir`, `ReadFile`.
- `internal/compiler/compiler_test.go` covers end-to-end: `TestStdlibStringsFixtureProgram`, `TestStdlibStringsExtFixtureProgram`, `TestStdlibUTF8FixtureProgram`, `TestStdlibConvFixtureProgram`, `TestStdlibFSPathFixtureProgram`, and `TestStdlibProcessEnvFixtureProgram` compile and run programs using stdlib functions.
- `TestUnhandledHostFilesystemErrorMain`, `TestUnhandledHostProcessErrorMain`, and `TestUnhandledHostProcessInvalidArgumentMain` verify that propagated host failures surface stable error names at the native `main` wrapper.
- `TestLocalPackageShadowsStdlib` verifies the shadowing behavior.
- `testdata/stdlib_strings/main.yar`, `testdata/stdlib_strings_ext/main.yar`, `testdata/stdlib_utf8/main.yar`, `testdata/stdlib_conv/main.yar`, `testdata/stdlib_fs_path/main.yar`, and `testdata/stdlib_process_env/main.yar` are the representative fixtures.
