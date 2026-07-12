# Standard Library

## Design

- The standard library is written in Yar, not host-language code.
- Stdlib packages are embedded into the Rust compiler with `include_str!` from
  `crates/yar-compiler/src/package.rs`.
- Stdlib packages use compiler-owned paths such as `import "std/strings"`.
- The `std/...` namespace resolves to embedded packages before any project or
  dependency lookup. Dependency aliases cannot use the `std` root.
- Stdlib packages use the same canonical namespace internally, so project or
  dependency packages cannot replace transitive stdlib packages.
- Bare user packages may use names such as `strings` or `fs`. If no user-owned
  package or declared alias resolves a bare known stdlib name, the compiler
  reports the required `std/...` migration path instead of falling back.
- Package identity includes origin and source-relative subpath; stdlib and
  non-stdlib packages with the same logical path can coexist safely.
- Stdlib packages are parsed, type-checked, and compiled through the same
  pipeline as user code.
- Most stdlib functions are ordinary Yar code. A small set of embedded `fs`,
  `process`, `env`, `stdio`, and `net` declarations are tagged as host
  intrinsics during checking and code generation and lower to runtime shims
  while keeping the user-facing API package-shaped.

## Infrastructure

- `crates/yar-compiler/src/package.rs` provides the stdlib lookup table.
- `stdlib/packages/<pkg>/<file>.yar` is the canonical location for
  stdlib source files.
- The Rust package loader strips the public `std/` prefix before calling
  `read_stdlib_package`, preserving bare internal package identities used by
  lowering and host-intrinsic dispatch.
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
- `trim(s str, cutset str) str` — strip leading and trailing bytes in cutset
- `split(s str, sep str) []str` — split string by separator; empty separator
  splits into individual bytes
- `to_lower(s str) str` — ASCII lowercase conversion
- `to_upper(s str) str` — ASCII uppercase conversion
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

All three helpers use simple in-place insertion sort written in Yar itself.

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
- `File { handle i64 }` — resource wrapper; it cannot cross a spawn boundary

Functions:

- `read_file(path str) !str`
- `write_file(path str, data str) !void`
- `read_dir(path str) ![]DirEntry`
- `stat(path str) !EntryKind`
- `mkdir_all(path str) !void`
- `remove_all(path str) !void`
- `temp_dir(prefix str) !str`
- `open_read(path str) !File`
- `open_write(path str) !File`

Methods on `File`:

- `read(max_bytes i32) !str` — read up to `max_bytes`; returns empty string on
  EOF
- `write(data str) !i32` — write data and return bytes written
- `close() !void` — close the file handle

File handles are positive, process-local registry IDs rather than native
addresses. Access is synchronized. Closing removes the ID so new lookups fail,
then waits for an operation holding the file lock before releasing the host
file; it does not interrupt blocking I/O. Unknown, stale, and wrong-kind IDs
produce `error.Closed`.

Errors:

- `error.NotFound`
- `error.PermissionDenied`
- `error.AlreadyExists`
- `error.InvalidPath`
- `error.InvalidArgument`
- `error.Closed`
- `error.IO`

### `io`

Stream interfaces and small stream helpers.

Interfaces:

- `Reader { read(max_bytes i32) !str }`
- `Writer { write(data str) !i32 }`
- `Closer { close() !void }`
- `ReadCloser { read(max_bytes i32) !str, close() !void }`
- `WriteCloser { write(data str) !i32, close() !void }`
- `ReadWriter { read(max_bytes i32) !str, write(data str) !i32 }`

Functions:

- `copy(dst Writer, src Reader, chunk_size i32) !i64` — copy from `src` to
  `dst` in bounded chunks
- `read_all(src Reader, chunk_size i32, max_bytes i32) !str` — read a stream
  into a string up to an explicit maximum
- `close_quiet(c Closer) void` — close and ignore the close error

Errors:

- `error.InvalidArgument`
- `error.LimitExceeded`
- `error.IO`

### `process`

Host-backed process and argv helpers.

Types:

- `Result { exit_code i32, stdout str, stderr str }`
- `Limits { timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64 }`
- `Cancellation { signal chan[bool] }` — share-safe close-only signal

Functions:

- `args() []str` — return the host-provided argument vector, including `argv[0]`
- `limits(timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64) !Limits`
- `cancellation() Cancellation` and `cancel(Cancellation) void`
- `run(argv []str, limits Limits, cancellation Cancellation) !Result` — run
  with a deadline and independent stdout/stderr caps
- `run_inherit(argv []str, timeout_milliseconds i64, cancellation Cancellation) !i32` —
  run with inherited stdio under a deadline and cancellation signal

Errors:

- `error.NotFound`
- `error.PermissionDenied`
- `error.InvalidArgument`
- `error.Timeout`
- `error.LimitExceeded`
- `error.Cancelled`
- `error.IO`

Timeouts range from 1 millisecond through 24 hours. Capture caps range from 0
through 64 MiB per stream, with the exact cap allowed. Timeout, cancellation,
or a cap breach terminates and reaps ordinary descendants before returning;
partial capture is discarded and cleanup failure becomes `error.IO`. Unix
descendants that create a new session may escape containment. Calls block only
their calling native task thread and provide no CPU, address-space, file,
network, or process-count sandbox.

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

### `net`

Host-backed TCP networking primitives.

Types:

- `Addr { host str, port i32 }`
- `Conn` — opaque typed, share-safe registry reference
- `Listener` — opaque typed, share-safe registry reference

Functions:

- `listen_stream(host str, port i32) !Listener` — bind and listen; empty host is
  the IPv4 wildcard address
- `connect_stream(host str, port i32) !Conn` — synchronous DNS resolution and TCP
  connection creation
- `resolve(host str, port i32) !Addr` — return the first IPv4 or IPv6 result

Methods on `Listener`:

- `accept() !Conn`
- `addr() !Addr`
- `close() !void`

Methods on `Conn`:

- `read(max_bytes i32) !str`
- `write(data str) !i32` — one host write returning its exact, possibly short,
  byte count
- `close() !void`
- `local_addr() !Addr`
- `remote_addr() !Addr`
- `set_read_deadline(millis i32) !void`
- `set_write_deadline(millis i32) !void`

Errors:

- `error.ConnectionRefused`
- `error.Timeout`
- `error.AddrInUse`
- `error.ConnectionReset`
- `error.NotFound` (DNS failure)
- `error.PermissionDenied`
- `error.InvalidArgument`
- `error.IO`
- `error.Closed`

`read` accepts 1 through 67,108,864 bytes inclusive and returns an empty string
only on EOF. One reader and one writer may operate concurrently; calls in the
same direction serialize. Close linearizes at registry removal, wakes blocked
accept/read/write calls with `error.Closed`, then waits for in-flight operations
and resource release. Raw `i64` network intrinsics are internal.

Read and write deadlines are relative per-operation socket timeouts. Zero
disables a timeout; changing it is not promised to interrupt a syscall already
in progress. Synchronous DNS and connect cannot be interrupted before a handle
exists. Resolver failure is `error.NotFound`.

### `testing`

Test framework for `yar test`.

Types:

- `T { name str, failed bool, messages []str }`

Methods:

- `fail(msg str) void` — mark test failed with message
- `log(msg str) void` — record a message
- `has_failed() bool` — check failure status

Functions:

- `equal[V](t *T, got V, want V) void` — equality assertion with "got X, want Y" message via `to_str`
- `not_equal[V](t *T, got V, want V) void` — inequality assertion
- `is_true(t *T, value bool) void`
- `is_false(t *T, value bool) void`
- `fail(t *T, msg str) void` — explicit failure with message

## Constraints

- Performance is straightforward and correctness-first. Concatenation-heavy
  functions like `repeat`, `replace`, `itoa`, and `itoa64` are O(n^2) for
  large inputs, and `sort` uses O(n^2) insertion sort.
- The Rust `fs`, `process`, and `net` runtime boundaries use `#[cfg(...)]`
  platform modules to support both POSIX and Windows implementations.
  On POSIX, the implementations use `stat`, `opendir`, `mkdir`, `remove`,
  `fork`, `execvp`, `waitpid`, `mkstemp`, and BSD sockets (`socket`, `bind`,
  `listen`, `accept`, `connect`, `recv`, `send`, `getaddrinfo`). On Windows,
  the implementations use Win32 APIs (`CreateFileA`, `FindFirstFileA`,
  `CreateDirectoryA`, `CreateProcessA`, `GetEnvironmentVariableA`, and
  Winsock2 functions). Windows runtime bundles include the complete ordered
  native-library contract for the Rust staticlib, including `ws2_32`.
- The `net` package exposes typed share-safe references backed by kind-checked,
  non-reused registry IDs. Raw IDs remain internal. Operations block only their
  native task thread. Close wakes blocked socket operations before waiting for
  cleanup. The runtime polls nonblocking sockets with adaptive bounded waits
  against per-operation relative timeouts; this portable native-thread model is
  not a high-scale readiness poller. SIGPIPE is suppressed on POSIX
  (`signal(SIGPIPE, SIG_IGN)` and `SO_NOSIGPIPE` on macOS).
- Process execution requires at least one argv element. Empty command vectors,
  invalid host strings, invalid timeouts, and invalid capture caps surface
  `error.InvalidArgument`.
- `fs.temp_dir` rejects prefixes containing path separators or embedded NUL
  bytes and creates directories under `TMPDIR` or `/tmp` on POSIX, or under
  the system temporary directory on Windows.
