# Proposal: TCP Networking (`net` stdlib package)

Status: accepted

## 1. Summary

Add a `net` stdlib package providing TCP client/server primitives: listen,
accept, connect, read, write, close, address inspection, timeouts, and DNS
resolution. Connections and listeners are represented as opaque `i64` handles.
All calls are blocking.

## 2. Motivation

Yar programs can read files, run processes, and inspect the environment, but
cannot communicate over the network. Even a simple TCP echo server or HTTP
client requires networking primitives. Without them, any networked program must
shell out to `curl` or similar tools via `process.run`.

Adding TCP support as a host-backed stdlib package follows the same pattern
established by `fs`, `process`, `env`, and `stdio`, keeping the runtime small
and the user-facing API idiomatic.

## 3. User-Facing Examples

### Valid examples

```
import "net"

// TCP echo server
fn main() !i32 {
    ln := net.listen("127.0.0.1", 8080)?
    for true {
        conn := net.accept(ln)?
        for true {
            data := net.read(conn, 4096)?
            if len(data) == 0 { break }
            net.write(conn, data)?
        }
        net.close(conn)?
    }
    return 0
}
```

```
import "net"

// TCP client
fn main() !i32 {
    conn := net.connect("example.com", 80)?
    net.write(conn, "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?
    response := net.read(conn, 65536)?
    print(response)
    net.close(conn)?
    return 0
}
```

```
import "net"

// Port 0 with address discovery
fn main() !i32 {
    ln := net.listen("127.0.0.1", 0)?
    addr := net.listener_addr(ln)?
    print("listening on port " + to_str(addr.port) + "\n")
    net.close_listener(ln)?
    return 0
}
```

### Invalid examples

```
// Cannot use listener handle as connection handle
ln := net.listen("127.0.0.1", 8080)?
net.read(ln, 4096)?  // runtime error: ln is a listener, not a connection
```

Both listeners and connections are `i64`, so misuse is not caught at compile
time. This is consistent with how `sb_new`/`sb_write`/`sb_string` handles work.

## 4. Semantics

- `listen(host, port)` creates a TCP listener bound to the given address. Empty
  host means all interfaces. Port 0 lets the OS assign a port. Returns an opaque
  `i64` listener handle.
- `accept(listener)` blocks until a client connects and returns an opaque `i64`
  connection handle.
- `listener_addr(listener)` returns the bound `Addr` of a listener (useful after
  port 0).
- `close_listener(listener)` closes a listener socket.
- `connect(host, port)` performs DNS resolution and TCP connect. Returns an
  opaque `i64` connection handle.
- `read(conn, max_bytes)` reads up to `max_bytes` from a connection. Returns
  empty string `""` on EOF (not an error). The runtime allocates the result via
  `yar_alloc`.
- `write(conn, data)` writes all bytes in `data`. Returns bytes written as
  `i32`.
- `close(conn)` closes a connection socket.
- `local_addr(conn)` / `remote_addr(conn)` return connection endpoint addresses.
- `set_read_deadline(conn, millis)` / `set_write_deadline(conn, millis)` set
  socket timeouts. 0 means no timeout.
- `resolve(host, port)` performs DNS resolution and returns the first result.

All functions that can fail return errorable types (`!i64`, `!str`, `!i32`,
`!void`, `!Addr`).

## 5. Type Rules

- `Addr` is a public struct: `pub struct Addr { host str, port i32 }`.
- Listener and connection handles are `i64`. No new type constructors.
- Port must be `i32` (valid range 0–65535, validated at runtime).
- Host must be `str`.
- `max_bytes` for `read` must be `i32` (positive, validated at runtime).
- `millis` for deadlines must be `i32` (non-negative, validated at runtime).
- All host-backed functions are errorable.

## 6. Grammar / Parsing Shape

No language grammar changes. The `net` package is a stdlib package imported with
`import "net"`. All functions are called with qualified names (`net.listen`,
`net.read`, etc.).

## 7. Lowering / Implementation Model

### Parser impact

None.

### AST / IR impact

None.

### Checker impact

- `IsHostIntrinsic` extended with all `net.*` function names.
- `registerHostErrorNames` extended to register 9 error names:
  `AddrInUse`, `Closed`, `ConnectionRefused`, `ConnectionReset`, `IO`,
  `InvalidArgument`, `NotFound`, `PermissionDenied`, `Timeout`.

### Codegen impact

- 13 new LLVM extern declarations (`@yar_net_*`) in `writeRuntimeDecls`.
- 14 new cases in `genHostIntrinsicCall` following existing patterns
  (alloca out → call → load → emit status result).
- 1 new case in `emitHostErrorCode` mapping 9 status codes to error names.

### Runtime impact

- ~400 lines of new C code in `runtime_source.txt`.
- BSD sockets on POSIX, Winsock2 on Windows.
- `yar_net_ensure_init()` for one-time Winsock/SIGPIPE setup.
- Windows builds link `-lws2_32`.

## 8. Interactions

### Errors

New error names (`ConnectionRefused`, `Timeout`, `AddrInUse`,
`ConnectionReset`, `Closed`) join the program-wide error code table. Existing
names (`IO`, `NotFound`, `PermissionDenied`, `InvalidArgument`) are reused.
All error handling works through standard `?` and `or |err| { ... }`.

### Structs

`net.Addr { host str, port i32 }` is a regular public struct. Field access,
literals, and passing work as expected.

### Control flow

All networking calls block. In a single-threaded program, `accept` blocks the
entire program until a connection arrives. No new control flow mechanisms.

### Builtins

No new builtins. `len`, `to_str`, etc. work on `net.Addr` fields naturally.

### Future concurrency

If Yar adds concurrency primitives, the blocking socket model naturally extends
to per-goroutine/thread blocking. The opaque handle approach does not preclude
this.

## 9. Alternatives Considered

### 1. Struct-based handles (`Listener`, `Conn` types)

Would provide compile-time type safety between listeners and connections.
Rejected because methods on opaque resource handles do not work well in Yar
(methods require named local struct types with value/pointer receivers), and
the additional struct layer adds boilerplate without significant benefit.

### 2. Combined "host:port" string addressing

Would match Go's `net.Dial("tcp", "host:port")` pattern. Rejected because
parsing "host:port" in C is fragile (IPv6 brackets, edge cases), and separate
parameters are more explicit, matching Yar's design philosophy.

### 3. Non-blocking I/O with polling

Would enable multiplexed servers. Rejected because Yar is single-threaded with
no async primitives. Blocking I/O is simpler and sufficient for the current
execution model.

## 10. Complexity Cost

- **Language surface**: zero (no grammar changes)
- **Parser complexity**: zero
- **Checker complexity**: minimal (2 switch cases added)
- **Codegen complexity**: moderate (14 new intrinsic cases, follows existing
  patterns exactly)
- **Runtime complexity**: moderate (~400 lines of C, cross-platform sockets)
- **Diagnostics complexity**: zero
- **Test burden**: 1 integration test
- **Documentation burden**: moderate (stdlib docs, error model, runtime docs)

## 11. Why Now?

Networking is the primary missing capability preventing Yar programs from being
useful for real-world tasks. The `fs`, `process`, and `env` packages established
the host-intrinsic pattern; `net` follows the same architecture with no new
compiler infrastructure. This unblocks HTTP servers, protocol implementations,
and networked tools.

## 12. Open Questions

- **UDP support**: deferred to a future proposal. TCP covers the primary use
  cases.
- **Connection pooling / keep-alive**: not in scope. Users manage connections
  explicitly.
- **TLS**: not in scope. Would require linking OpenSSL or similar, significantly
  increasing complexity.

## 13. Decision

Accepted. TCP networking fills the most impactful gap in Yar's stdlib with
minimal compiler complexity, following established patterns.

## 14. Implementation Checklist

- [x] `internal/stdlib/packages/net/net.yar` — Yar source declarations
- [x] `internal/checker/checker.go` — host intrinsic registration, error names
- [x] `internal/codegen/llvm.go` — LLVM declarations, intrinsic codegen, error
      mapping
- [x] `internal/runtime/runtime_source.txt` — C runtime implementation
- [x] `internal/compiler/cc.go` — Windows `-lws2_32` linking
- [x] `testdata/stdlib_net/main.yar` — integration test fixture
- [x] `internal/compiler/compiler_test.go` — test function
- [x] `docs/context/domains/stdlib.md` — net package documentation
- [x] `docs/context/domains/error-model.md` — net error names
- [x] `docs/context/platform/toolchain-runtime.md` — net runtime surface
- [x] `docs/context/summary.md` — updated stdlib list
- [x] `docs/YAR.md` — net package reference
- [x] `docs/context/domains/language-slice.md` — updated stdlib list
