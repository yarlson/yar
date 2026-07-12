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

### `net`

Host-backed TCP networking primitives.

Types:

- `Addr { host str, port i32 }`
- `Conn { handle i64 }` — resource wrapper; it cannot cross a spawn boundary
- `Listener { handle i64 }` — resource wrapper; it cannot cross a spawn boundary

Functions:

- `listen(host str, port i32) !i64` — bind and listen on a TCP address; empty
  host means all interfaces; returns an opaque listener handle
- `accept(listener i64) !i64` — block until a connection arrives on the
  listener; returns an opaque connection handle
- `listener_addr(listener i64) !Addr` — return the bound address of a listener
  (useful for OS-assigned port discovery)
- `close_listener(listener i64) !void` — close a listener socket
- `connect(host str, port i32) !i64` — TCP connect with DNS resolution; returns
  an opaque connection handle
- `read(conn i64, max_bytes i32) !str` — read up to `max_bytes`; returns empty
  string on EOF
- `write(conn i64, data str) !i32` — write all of data; returns bytes written
- `close(conn i64) !void` — close a connection
- `local_addr(conn i64) !Addr` — local address of a connection
- `remote_addr(conn i64) !Addr` — remote address of a connection
- `set_read_deadline(conn i64, millis i32) !void` — set read timeout in
  milliseconds; 0 disables the timeout
- `set_write_deadline(conn i64, millis i32) !void` — set write timeout in
  milliseconds; 0 disables the timeout
- `resolve(host str, port i32) !Addr` — DNS resolution; returns the first
  resolved address
- `listen_stream(host str, port i32) !Listener` — listen and wrap the listener
  handle
- `connect_stream(host str, port i32) !Conn` — connect and wrap the connection
  handle

Methods on `Listener`:

- `accept() !Conn`
- `addr() !Addr`
- `close() !void`

Methods on `Conn`:

- `read(max_bytes i32) !str`
- `write(data str) !i32`
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

### `http`

Pure-Yar HTTP/1.1 server helpers built on top of `net`.

Types:

- `Request { method str, path str, headers map[str]str, body str }`
- `Response { status i32, headers map[str]str, body str }`

Functions:

- `text(status i32, body str) Response` — create a text/plain response with a
  UTF-8 content type
- `serve(addr net.Addr, handler fn(Request) !Response) !void` — listen on a
  TCP address, accept connections, parse one HTTP request per connection, call
  the handler, write one response, and close the connection

Semantics:

- Connections are processed sequentially; `serve` accepts the next connection
  after the current connection is closed.
- Request header names are normalized to lowercase.
- `Content-Length` is honored for bodies up to 65536 bytes. Larger or invalid
  lengths return `error.InvalidRequest` inside the package.
- Handler errors are converted to `500 Internal Server Error`; the connection
  is closed and `serve` continues accepting.
- Malformed requests receive `400 Bad Request`; the connection is closed.
- The response writer always sets `content-length` and defaults
  `content-type` to `text/plain; charset=utf-8` when absent.

Current v1 non-goals:

- no keep-alive
- no router
- no query parser
- no middleware
- no stdlib auth
- no TLS

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
- The `net` package uses opaque `i64` handles for listeners and connections.
  They are kind-checked, non-reused process-local registry IDs, and their state
  is synchronized. Explicit close removes the ID so new lookups fail, then
  waits for an operation holding the socket lock before releasing it; close
  does not interrupt blocking I/O. Unknown, stale, and wrong-kind IDs produce
  `error.Closed`. All networking calls are blocking. Timeouts are set
  per-connection via `SO_RCVTIMEO`/`SO_SNDTIMEO`. SIGPIPE is suppressed on POSIX
  (`signal(SIGPIPE, SIG_IGN)` and `SO_NOSIGPIPE` on macOS).
- `process.run` and `process.run_inherit` require at least one argv element.
  Empty command vectors and strings that cannot cross the host boundary surface
  `error.InvalidArgument`.
- `fs.temp_dir` rejects prefixes containing path separators or embedded NUL
  bytes and creates directories under `TMPDIR` or `/tmp` on POSIX, or under
  the system temporary directory on Windows.
