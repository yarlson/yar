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
  `strings.InvalidInteger` or `strings.IntegerOverflow`

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

- `utf8.InvalidUTF8`
- `utf8.OutOfRange`

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

- `DirEntry { pub name str, pub is_dir bool }`
- `EntryKind { File, Directory, Other }`
- `File` — package-owned resource wrapper with a private handle; it cannot cross
  a spawn boundary

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

- `fs.NotFound`
- `fs.PermissionDenied`
- `fs.AlreadyExists`
- `fs.InvalidPath`
- `fs.InvalidArgument`
- `error.Closed`
- `fs.IO`

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

- `io.InvalidArgument`
- `io.LimitExceeded`
- `io.IO`

Errors propagated from supplied stream implementations retain their original
package-owned identity.

### `process`

Host-backed process and argv helpers.

Types:

- `Result { pub exit_code i32, pub stdout str, pub stderr str }`
- `Limits` — package-owned validated limits with private fields
- `Cancellation` — package-owned share-safe close-only signal with a private
  channel field

Functions:

- `args() []str` — return the host-provided argument vector, including `argv[0]`
- `limits(timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64) !Limits`
- `cancellation() Cancellation` and `cancel(Cancellation) void`
- `run(argv []str, limits Limits, cancellation Cancellation) !Result` — run
  with a deadline and independent stdout/stderr caps
- `run_inherit(argv []str, timeout_milliseconds i64, cancellation Cancellation) !i32` —
  run with inherited stdio under a deadline and cancellation signal

Errors:

- `process.NotFound`
- `process.PermissionDenied`
- `process.InvalidArgument`
- `process.Timeout`
- `process.LimitExceeded`
- `process.Cancelled`
- `process.IO`

Timeouts range from 1 millisecond through 24 hours. Capture caps range from 0
through 64 MiB per stream, with the exact cap allowed. Timeout, cancellation,
or a cap breach terminates and reaps ordinary descendants before returning;
partial capture is discarded and cleanup failure becomes `process.IO`. Unix
descendants that create a new session may escape containment. Calls block only
their calling native task thread and provide no CPU, address-space, file,
network, or process-count sandbox.

### `env`

Host-backed environment lookup.

Functions:

- `lookup(name str) !str` — return one environment variable value, or
  `env.NotFound` when absent

Additional current failure mode:

- `env.InvalidArgument` for names that cannot cross the host boundary
- `env.PermissionDenied` and `env.IO` for other host failures

### `stdio`

Host-backed stderr output.

Functions:

- `eprint(msg str) void` — write one string to stderr

### `net`

Host-backed TCP networking primitives.

Types:

- `Addr { pub host str, pub port i32 }`
- `Conn` — package-owned typed, share-safe registry reference with a private
  handle field
- `Listener` — package-owned typed, share-safe registry reference with a private
  handle field

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
- `shutdown_write() !void` — finish output while preserving reads

Methods on `Conn`:

- `read(max_bytes i32) !str`
- `write(data str) !i32` — one host write returning its exact, possibly short,
  byte count
- `close() !void`
- `local_addr() !Addr`
- `remote_addr() !Addr`
- `set_read_deadline(millis i32) !void`
- `set_write_deadline(millis i32) !void`
- `set_read_deadline_after(millis i32) !void`
- `set_write_deadline_after(millis i32) !void`

Errors:

- `net.ConnectionRefused`
- `net.Timeout`
- `net.AddrInUse`
- `net.ConnectionReset`
- `net.NotFound` (DNS failure)
- `net.PermissionDenied`
- `net.InvalidArgument`
- `net.IO`
- `error.Closed`

`read` accepts 1 through 67,108,864 bytes inclusive and returns an empty string
only on EOF. One reader and one writer may operate concurrently; calls in the
same direction serialize. Close linearizes at registry removal, wakes blocked
accept/read/write calls with `error.Closed`, then waits for in-flight operations
and resource release. Raw `i64` network intrinsics are internal.

Read and write deadlines are relative per-operation socket timeouts. Zero
disables a timeout; changing it is not promised to interrupt a syscall already
in progress. The `*_deadline_after` variants instead anchor one fixed deadline
at the setter call, shared by all later operations in that direction; zero
disables it. When both are set, the earlier per-operation or fixed deadline
wins. Synchronous DNS and connect cannot be interrupted before a handle exists.
Resolver failure is `net.NotFound`.

### `http`

Pure-Yar bounded HTTP/1.1 server handling over `net`.

Public types:

- `Header { pub name str, pub value str }`
- `Request { pub method str, pub target str, pub headers []Header, pub body str, pub path_values []url.Param }`
- `Limits` — validated private framing and deadline limits
- `Response` — validated private status, headers, and body
- `Server` — private typed listener plus limits
- `Connection` — private typed connection plus limits
- `Route` — private validated method, pattern segments, and handler
- `Router` — private conflict-free route list

Functions:

- `default_limits() Limits`
- `limits(max_head_bytes, max_body_bytes, read_timeout_millis, write_timeout_millis) !Limits`
- `listen(addr net.Addr, limits Limits) !Server`
- `response(status i32, body str) !Response`
- `text(status i32, body str) !Response`
- `json(status i32, body str) !Response`
- `route(method str, pattern str, handler fn(Request) !Response) !Route`
- `router(routes []Route) !Router`

Methods:

- `Request.header(name) !str` and `header_values(name) ![]str`
- `Request.path() str`, `query() !url.Query`, and `path_value(name) !str`
- `Response.status() i32`, `body() str`, and `header(name) !str`
- `Router.serve(req Request) !Response`
- `Response.with_header(name, value) !Response` replaces a field
- `Response.add_header(name, value) !Response` preserves repeated fields
- `Server.accept() !Connection`, `addr() !net.Addr`, and `close() !void`
- `Connection.serve(handler fn(Request) !Response) !void`, address accessors,
  and `close() !void`

Each served connection processes one request and closes. Parsing is incremental
across TCP reads, requires strict CRLF and HTTP/1.1 grammar, supports bounded
`Content-Length` and `chunked` bodies, rejects ambiguous framing, and validates
targets, authorities, chunk extensions, and trailers. Absolute-form authority
replaces the received Host field before handler dispatch. Fixed read/write
deadlines bound the exchange. Protocol errors receive bodyless bounded responses
and remain visible to the caller as package-owned errors.
Response framing belongs to the package; application headers cannot override
`Content-Length`, transfer coding, connection, trailer, or upgrade fields.

The router (`router.yar`) splits `Request.path()` into percent-decoded
segments, filters routes by method (`HEAD` falls back to `GET`), and picks the
most specific match by comparing segment kinds left to right: literal, then
`{name}`, then a final `{name...}`. Path-only matches produce `405` with
`Allow`; other misses produce `404`; bad escapes produce `400`. `router`
rejects routes with the same method and segment shape. Routers hold function
values and are not share-safe.

The caller owns the accept loop, concurrency, cancellation, logging, and error
policy. HTTP clients, TLS, keep-alive, upgrades, compression, middleware, and
streaming body APIs are not part of the package.

### `url`

Pure-Yar percent-encoding and form-query parsing: `percent_decode`,
`percent_encode`, and `parse_query` returning an ordered `Query` with `get`,
`values`, and `params`. Errors are `url.InvalidEscape` and `url.NotFound`.

### `json`

Pure-Yar JSON as the public enum `Value` (`Null`, `Bool`, `Number` with
validated text, `String`, `Array`, `Object` with ordered `Member`s). `parse`
is a strict recursive-descent parser over byte offsets with a nesting limit of
128; `encode` writes compact JSON through a string builder and validates
caller-built numbers, UTF-8, duplicate names, and depth. Accessors (`get`,
`as_*`, `is_null`) and constructors (`int`, `number`) report
`json.TypeMismatch`, `json.NotFound`, `json.OutOfRange`, or
`json.InvalidNumber`.

### `time`

Nominal `Timestamp`, `Instant`, and `Duration` structs with private `i64`
nanosecond fields, plus the transparent UTC `Date`. Three private host
intrinsics (`now_unix_nanoseconds`, `instant_nanoseconds`,
`sleep_nanoseconds`) back `now`, `instant`, and `sleep`; everything else is
pure Yar: checked `i64` arithmetic, floor division, Hinnant civil-day
conversion, strict field validation, and fixed-width RFC 3339 text. Negative
timestamps with fractions convert through `seconds + 1` so the earliest
representable nanosecond does not overflow. Errors are `time.InvalidArgument`,
`time.InvalidFormat`, and `time.Overflow`.

### `testing`

Test framework for `yar test`.

Types:

- `T` — package-owned test state with private fields

Methods:

- `fail(msg str) void` — mark test failed with message
- `log(msg str) void` — record a message
- `has_failed() bool` — check failure status
- `message_count() i32` — return the number of recorded messages
- `message(index i32) str` — return one recorded message

Functions:

- `new(name str) *T` — construct the private test state used by the generated
  runner
- `equal[V](t *T, got V, want V) void` — equality assertion with "got X, want Y" message via `to_str`
- `not_equal[V](t *T, got V, want V) void` — inequality assertion
- `is_true(t *T, value bool) void`
- `is_false(t *T, value bool) void`
- `fail(t *T, msg str) void` — explicit failure with message

## Constraints

- Performance is straightforward and correctness-first. Concatenation-heavy
  functions like `repeat`, `replace`, `itoa`, and `itoa64` are O(n^2) for
  large inputs, and `sort` uses O(n^2) insertion sort.
- The Rust runtime uses `std::fs`, `std::path`, and `std::env` for portable
  filesystem and environment behavior, `std::net` for TCP, and the shared
  `yar-process-control` crate for child execution. Platform-specific code owns
  only the contracts that differ: Unix path bytes, process groups and signals,
  Windows path conversion and Job Objects, and GC stack discovery. Runtime
  bundles carry the complete ordered native-library contract for each Rust
  static library target, including `ws2_32` on Windows.
- The `net` package exposes typed share-safe references backed by kind-checked,
  generation-tagged registry tokens. Vacant slots may be reused, but their new
  generation changes the full token and leaves stale generations invalid. Raw
  IDs remain internal. Operations block only their
  native task thread. Close wakes blocked socket operations before waiting for
  cleanup. The runtime polls nonblocking sockets with adaptive bounded waits
  against per-operation relative timeouts; this portable native-thread model is
  not a high-scale readiness poller.
- Process execution requires at least one argv element. Empty command vectors,
  invalid host strings, invalid timeouts, and invalid capture caps surface
  `process.InvalidArgument`.
- `fs.temp_dir` rejects prefixes containing path separators or embedded NUL
  bytes and creates directories under `TMPDIR` or `/tmp` on POSIX, or under
  the system temporary directory on Windows.
