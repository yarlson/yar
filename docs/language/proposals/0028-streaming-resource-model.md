# Proposal: Streaming Resource Model (`io`, `fs`, and `net`)

Status: accepted and implemented

## 1. Summary

Add a small streaming/resource foundation for Yar programs.

The model starts in the standard library rather than as new syntax:

- `io` defines `Reader`, `Writer`, `Closer`, and combined stream interfaces.
- `io.copy` copies between streams in bounded chunks.
- `io.read_all` reads a stream with an explicit byte limit.
- `fs.File` exposes streaming file reads, writes, and close.
- `net.Conn` and `net.Listener` wrap the existing TCP handles with methods that
  fit the same stream interfaces.

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

`copy` returns `error.InvalidArgument` when `chunk_size <= 0`, propagates stream
errors, and returns `error.IO` if a writer reports a short write.

`read_all` returns `error.InvalidArgument` for invalid limits and
`error.LimitExceeded` when reading would exceed `max_bytes`.

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

- `error.NotFound`
- `error.PermissionDenied`
- `error.AlreadyExists`
- `error.InvalidPath`
- `error.InvalidArgument`
- `error.Closed`
- `error.IO`

### `net`

The existing low-level `i64` functions stay available.

New wrappers:

```yar
pub struct Conn {
    handle i64
}

pub struct Listener {
    handle i64
}

pub fn listen_stream(host str, port i32) !Listener
pub fn connect_stream(host str, port i32) !Conn

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
  resources. They are kind-checked, non-reused process-local registry IDs, and
  their mutable state is synchronized.
- Explicit file and network close removes the registry ID so new lookups fail,
  then waits for any operation holding the per-resource lock before releasing
  the host resource; close does not interrupt blocking I/O. Unknown, stale, and
  wrong-kind IDs produce `error.Closed`.
- Registry validation is a runtime safety boundary. Raw `i64` handles still
  have no compiler-visible nominal type or provenance.

This proposal does not introduce automatic cleanup. Programs are responsible
for calling `close`.

## 5. Implementation Notes

The `io` package is pure Yar.

`fs.File` uses private host intrinsics:

- `fs.open_read_handle(path) !i64`
- `fs.open_write_handle(path) !i64`
- `fs.read_handle(handle, max_bytes) !str`
- `fs.write_handle(handle, data) !i32`
- `fs.close_handle(handle) !void`

The runtime stores file stream state behind validated registry IDs. Closing
removes the ID; later lookup of that stale ID reports `error.Closed`. IDs are
never reused within the process, so a stale handle cannot resolve to a newer
resource.

The `net` additions are pure Yar wrappers around existing networking intrinsics.

## 6. Tests

`testdata/stdlib_io/main.yar` validates:

- streaming file copy through `io.copy`
- explicit close on both streams
- `error.Closed` after using a closed file
- `error.InvalidArgument` for invalid copy chunk size
- `error.LimitExceeded` for bounded `io.read_all`
- whole-file verification through the existing `fs.read_file`

`TestStdlibIOFixtureProgram` builds and runs the fixture through the native
compiler/runtime boundary.

## 7. Non-Goals

- no `defer`
- no RAII/destructors
- no ownership or linear types
- no async/await
- no nonblocking descriptor API
- no file descriptor inheritance model
- no binary byte-buffer type
- no HTTP request/response streaming in this proposal
- no automatic resource leak detection
