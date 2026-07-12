# Proposal: Streaming Resource Model (`io`, `fs`, and `net`)

Status: accepted
Implementation: implemented

## 1. Summary

Add a small streaming/resource foundation for Yar programs.

The model starts in the standard library rather than as new syntax:

- `io` defines `Reader`, `Writer`, `Closer`, and combined stream interfaces.
- `io.copy` copies between streams in bounded chunks.
- `io.read_all` reads a stream with an explicit byte limit.
- `fs.File` exposes streaming file reads, writes, and close.
- `net.Conn` and `net.Listener` are typed, share-safe registry references with
  methods that fit the same stream interfaces. Raw network IDs are internal.

This is the foundation needed before HTTP grows beyond whole-request and
whole-response strings. It gives Yar a common stream vocabulary without adding
RAII, ownership, `defer`, async/await, or allocator syntax.

## 2. User-Facing API

### `io`

```yar
pub interface Reader {
    read(max_bytes i32) !str
}

pub interface Writer {
    write(data str) !i32
}

pub interface Closer {
    close() !void
}

pub interface ReadCloser {
    read(max_bytes i32) !str
    close() !void
}

pub interface WriteCloser {
    write(data str) !i32
    close() !void
}

pub interface ReadWriter {
    read(max_bytes i32) !str
    write(data str) !i32
}

pub fn copy(dst Writer, src Reader, chunk_size i32) !i64
pub fn read_all(src Reader, chunk_size i32, max_bytes i32) !str
pub fn close_quiet(c Closer) void
```

`read(max_bytes)` returns an empty string on EOF. This mirrors the existing
`net.read` contract and keeps EOF out of the ordinary error space.

`copy` returns `io.InvalidArgument` when `chunk_size <= 0`, propagates stream
errors, and returns `io.IO` if a writer reports a short write.

`read_all` returns `io.InvalidArgument` for invalid limits and
`io.LimitExceeded` when reading would exceed `max_bytes`.

### `fs`

```yar
pub struct File {
    handle i64
}

pub fn open_read(path str) !File
pub fn open_write(path str) !File

pub fn (f File) read(max_bytes i32) !str
pub fn (f File) write(data str) !i32
pub fn (f File) close() !void
```

`open_write` creates or truncates the destination file.

Filesystem stream failures use ordinary errors:

- `fs.NotFound`
- `fs.PermissionDenied`
- `fs.AlreadyExists`
- `fs.InvalidPath`
- `fs.InvalidArgument`
- `error.Closed`
- `fs.IO`

### `net`

`Conn` and `Listener` are public package-owned named structs. Their registry
fields are private under the general struct visibility rules, so external code
cannot select those fields or construct resource literals.

```yar
pub fn listen_stream(host str, port i32) !Listener
pub fn connect_stream(host str, port i32) !Conn
pub fn resolve(host str, port i32) !Addr

pub fn (l Listener) accept() !Conn
pub fn (l Listener) addr() !Addr
pub fn (l Listener) close() !void

pub fn (c Conn) read(max_bytes i32) !str
pub fn (c Conn) write(data str) !i32
pub fn (c Conn) close() !void
pub fn (c Conn) local_addr() !Addr
pub fn (c Conn) remote_addr() !Addr
pub fn (c Conn) set_read_deadline(millis i32) !void
pub fn (c Conn) set_write_deadline(millis i32) !void
```

`net.Conn` satisfies `io.Reader`, `io.Writer`, `io.Closer`,
`io.ReadCloser`, `io.WriteCloser`, and `io.ReadWriter`.

Connections allow one reader and one writer concurrently; calls in the same
direction serialize. `read` accepts at most 64 MiB, inclusive. `write` performs
one host write and returns its exact count, which may be short.

## 3. Example

```yar
package main

import "std/fs"
import "std/io"

fn main() !i32 {
    src := fs.open_read("input.txt")?
    dst := fs.open_write("output.txt")?

    io.copy(dst, src, 8192)?

    src.close()?
    dst.close()?
    return 0
}
```

## 4. Semantics

- Stream reads are blocking.
- Stream writes are blocking.
- An empty read result means EOF.
- `close` releases the underlying host resource.
- Using a closed file handle returns `error.Closed`.
- Allocation failure remains an unrecoverable runtime failure, not a Yar
  `error`.
- File stream handles are opaque `i64` values backed by runtime-managed host
  resources. They are positive, kind-checked, generation-tagged process-local
  registry tokens, and their mutable state is synchronized.
- File close removes the registry ID, waits for an operation holding its lock,
  and releases the host file without an implicit durability sync. It does not
  interrupt blocking file I/O.
- Network close linearizes at registry removal, wakes blocked accept/read/write
  operations with `error.Closed`, and waits for operation and resource release.
- Typed network resources are share-safe. Raw network IDs are compiler/runtime
  implementation details, not public stdlib values.

This proposal does not introduce automatic cleanup. Programs are responsible
for calling `close`.

## 5. Implementation Notes

The `io` package is pure Yar.

`fs.File`, `net.Conn`, and `net.Listener` use ordinary private struct fields for
representation hiding. The compiler does not attach stdlib-specific opacity
metadata.

`fs.File` uses private host intrinsics:

- `fs.open_read_handle(path) !i64`
- `fs.open_write_handle(path) !i64`
- `fs.read_handle(handle, max_bytes) !str`
- `fs.write_handle(handle, data) !i32`
- `fs.close_handle(handle) !void`

The runtime stores file stream state behind validated registry IDs. Closing
removes the token; later lookup of that stale generation reports `error.Closed`.
The vacant slot may be reused only after its generation advances, which changes
the full token and prevents a stale handle from resolving to a newer resource.
After the maximum generation is removed, the slot is retired instead of wrapped.

The `net` methods lower through compiler-internal networking intrinsics. Socket
timeouts are relative per-operation timeouts; changing one need not interrupt
an already-running syscall. DNS and connect are synchronous and cannot be
interrupted before a connection handle exists.

## 6. Tests

`testdata/stdlib_io/main.yar` validates:

- streaming file copy through `io.copy`
- explicit close on both streams
- `error.Closed` after using a closed file
- `io.InvalidArgument` for invalid copy chunk size
- `io.LimitExceeded` for bounded `io.read_all`
- whole-file verification through the existing `fs.read_file`

`TestStdlibIOFixtureProgram` builds and runs the fixture through the native
compiler/runtime boundary.

## 7. Non-Goals

- no `defer`
- no RAII/destructors
- no linear types
- no async/await
- no nonblocking descriptor API
- no file descriptor inheritance model
- no binary byte-buffer type
- no HTTP request/response streaming in this proposal
- no automatic resource leak detection
