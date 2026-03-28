# Standard Library

## Design

- The standard library is written in yar, not C or Go.
- Stdlib packages are embedded into the Go compiler binary using `go:embed` via
  the `internal/stdlib` package.
- Stdlib packages are imported with bare paths like any user package:
  `import "strings"`.
- Resolution order is local packages first, embedded stdlib second. A local
  directory with the same name shadows the stdlib package.
- Stdlib packages are parsed, type-checked, and compiled through the same
  pipeline as user code.
- Most stdlib functions are ordinary yar code. A small set of embedded `fs`,
  `process`, `env`, and `stdio` declarations are tagged as host intrinsics
  during checking and code generation and lower to runtime shims while keeping
  the user-facing API package-shaped.

## Infrastructure

- `internal/stdlib/stdlib.go` provides `Has`, `ReadDir`, and `ReadFile` for the
  package loader.
- `internal/stdlib/packages/<pkg>/<file>.yar` is the canonical location for
  stdlib source files.
- `internal/compiler/packages.go` calls `loadStdlibPackage` as a fallback when
  `os.ReadDir` fails with `os.ErrNotExist` for an import path.
- Both local and stdlib packages share the same import-resolution path.
- Stdlib packages can import other stdlib packages.
- Stdlib packages may use the internal builtins `chr`, `i32_to_i64`, and
  `i64_to_i32`. User code cannot call these names directly.

## Packages

### `strings`

Practical string operations built on the core string primitives (`len(str)`,
`s[i]`, `s[i:j]`, `==`, and `+`).

Functions:

- `contains(s str, substr str) bool` — linear scan with slice compare
- `has_prefix(s str, prefix str) bool` — compare prefix slice
- `has_suffix(s str, suffix str) bool` — compare suffix slice
- `index(s str, substr str) i32` — byte offset or -1
- `count(s str, substr str) i32` — non-overlapping occurrences
- `repeat(s str, n i32) str` — concatenation loop
- `replace(s str, old str, new str, n i32) str` — find-and-replace, `n < 0`
  means all
- `trim_left(s str, cutset str) str` — strip leading bytes in cutset
- `trim_right(s str, cutset str) str` — strip trailing bytes in cutset
- `join(parts []str, sep str) str` — join slice of strings
- `from_byte(i32) str` — construct a single-byte string
- `parse_i64(str) !i64` — parse a base-10 signed integer; returns
  `error.InvalidInteger` or `error.IntegerOverflow`

Internal helpers `contains_byte` and `parse_positive` are not exported.

### `utf8`

UTF-8 decoding and rune classification for lexers and diagnostic code.

Functions:

- `decode(s str, off i32) !i32` — decode the rune at byte offset `off`
- `width(s str, off i32) !i32` — byte width of the rune at byte offset `off`
- `is_letter(r i32) bool` — letter or underscore classification
- `is_digit(r i32) bool` — ASCII digit `0` through `9`
- `is_space(r i32) bool` — Unicode whitespace classification

Errors:

- `error.InvalidUTF8`
- `error.OutOfRange`

### `conv`

Type conversion and integer-to-string helpers.

Functions:

- `to_i64(n i32) i64`
- `to_i32(n i64) i32`
- `byte_to_str(b i32) str`
- `itoa(n i32) str`
- `itoa64(n i64) str`

### `sort`

Deterministic in-place sorting helpers for compiler and tooling code.

Functions:

- `strings(values []str) void` — ascending bytewise lexicographic order
- `i32s(values []i32) void` — ascending numeric order
- `i64s(values []i64) void` — ascending numeric order

All three helpers use simple in-place insertion sort written in yar itself.

### `path`

Pure path helpers for host-facing tooling code.

Functions:

- `clean(p str) str` — normalize `\` to `/`, collapse repeated separators, and
  simplify `.` / `..` segments
- `join(parts []str) str` — join path segments with `/` then clean the result
- `dir(p str) str` — parent path, or `.` when there is no separator
- `base(p str) str` — final path element
- `ext(p str) str` — suffix from the final `.`, or `""`

The implementation normalizes to forward slashes rather than emitting an
OS-specific separator.

### `fs`

Host-backed text-oriented filesystem helpers.

Types:

- `DirEntry { name str, is_dir bool }`
- `EntryKind { File, Directory, Other }`

Functions:

- `read_file(path str) !str`
- `write_file(path str, data str) !void`
- `read_dir(path str) ![]DirEntry`
- `stat(path str) !EntryKind`
- `mkdir_all(path str) !void`
- `remove_all(path str) !void`
- `temp_dir(prefix str) !str`

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

- `args() []str` — return the host-provided argument vector, including `argv[0]`
- `run(argv []str) !Result` — launch one child process, capture stdout/stderr,
  and return the child exit code plus captured output
- `run_inherit(argv []str) !i32` — launch one child process with inherited
  stdin/stdout/stderr and return the child exit code

Errors:

- `error.NotFound`
- `error.PermissionDenied`
- `error.InvalidArgument`
- `error.IO`

### `env`

Host-backed environment lookup.

Functions:

- `lookup(name str) !str` — return one environment variable value, or
  `error.NotFound` when absent

Additional current failure mode:

- `error.InvalidArgument` for names that cannot cross the host boundary

### `stdio`

Host-backed stderr output.

Functions:

- `eprint(msg str) void` — write one string to stderr

## Constraints

- Performance is straightforward and correctness-first. Concatenation-heavy
  functions like `repeat`, `replace`, `itoa`, and `itoa64` are O(n^2) for
  large inputs, and `sort` uses O(n^2) insertion sort.
- The `fs` runtime boundary is POSIX-oriented (`stat`, `opendir`, `mkdir`,
  `remove`, `TMPDIR`) rather than a full cross-platform abstraction.
- The `process` runtime boundary is POSIX-oriented (`fork`, `execvp`,
  `waitpid`, `mkstemp`) and captures child stdout/stderr through temporary
  files before copying them into runtime-managed strings.
- `process.run` and `process.run_inherit` require at least one argv element.
  Empty command vectors and strings that cannot cross the host boundary surface
  `error.InvalidArgument`.
- `fs.temp_dir` rejects prefixes containing path separators or embedded NUL
  bytes and creates directories under `TMPDIR` or `/tmp`.
