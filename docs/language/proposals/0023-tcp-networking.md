# Proposal: TCP Networking (`net` stdlib package)

Status: accepted
Implementation: implemented

## 1. Summary

Add a `net` stdlib package providing TCP client/server primitives: listen,
accept, connect, read, write, close, address inspection, timeouts, and DNS
resolution. Connections and listeners are public typed, opaque, share-safe
`Conn` and `Listener` references. The runtime backs them with kind-checked,
generation-tagged process-local opaque `i64` tokens. Vacant registry slots may
be reused, but reuse changes the full token and leaves stale generations
invalid. Stale-generation and wrong-kind access does not consume a current
entry, and maximum-generation slots are retired rather than wrapped. Raw IDs
exist only at the internal compiler/runtime ABI. Calls block only their calling
native task thread.

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
import "std/net"

fn write_all(conn net.Conn, data str) !void {
    off := 0
    for off < len(data) {
        n := conn.write(data[off:])?
        if n <= 0 { return error.IO }
        off += n
    }
    return
}

// TCP echo server
fn main() !i32 {
    ln := net.listen_stream("127.0.0.1", 8080)?
    for true {
        conn := ln.accept()?
        for true {
            data := conn.read(4096)?
            if len(data) == 0 { break }
            write_all(conn, data)?
        }
        conn.close()?
    }
    return 0
}
```

```
import "std/net"

fn write_all(conn net.Conn, data str) !void {
    off := 0
    for off < len(data) {
        n := conn.write(data[off:])?
        if n <= 0 { return error.IO }
        off += n
    }
    return
}

// TCP client
fn main() !i32 {
    conn := net.connect_stream("example.com", 80)?
    write_all(conn, "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?
    response := conn.read(65536)?
    print(response)
    conn.close()?
    return 0
}
```

```
import "std/net"

// Port 0 with address discovery
fn main() !i32 {
    ln := net.listen_stream("127.0.0.1", 0)?
    addr := ln.addr()?
    print("listening on port " + to_str(addr.port) + "\n")
    ln.close()?
    return 0
}
```

### Invalid examples

```
// A Listener is not a Conn.
ln := net.listen_stream("127.0.0.1", 8080)?
ln.read(4096)?  // checker error: Listener has no read method
```

The public types prevent listener/connection confusion. Compiler-internal IDs
remain kind-checked at the runtime boundary.

## 4. Semantics

- `listen_stream(host, port)` creates a typed TCP listener. Empty host means
  the IPv4 wildcard address. Port 0 lets the OS assign a port.
- `Listener.accept()` blocks until a client connects and returns a typed `Conn`.
- `Listener.addr()` returns the bound `Addr` (useful after port 0), and
  `Listener.close()` closes it.
- `connect_stream(host, port)` performs synchronous DNS resolution and TCP
  connect and returns a typed `Conn`.
- `Conn.read(max_bytes)` reads up to `max_bytes` from a connection. Returns
  empty string `""` on EOF (not an error). The runtime allocates the result via
  `yar_alloc`.
- `Conn.write(data)` performs one host write and returns its exact byte count;
  the result may be shorter than `len(data)`.
- `Conn.close()` closes a connection socket.
- `Conn.local_addr()` / `remote_addr()` return connection endpoint addresses.
- `Conn.set_read_deadline(millis)` / `set_write_deadline(millis)` set
  socket timeouts. 0 means no timeout.
- `resolve(host, port)` returns the first IPv4 or IPv6 result; resolver failure
  is `error.NotFound`.
- Typed connections and listeners are share-safe registry references. One read
  and one write may run concurrently; same-direction operations serialize.
- Closing linearizes at registry removal, wakes blocked accept/read/write calls
  with `error.Closed`, and waits for operation and resource release.

All functions and methods that can fail return errorable types.

## 5. Type Rules

- `Addr` is a public transparent struct with `pub host str` and `pub port i32`.
- Listener and connection values use public named `Listener` and `Conn` structs
  whose private registry fields make external selector access and literal
  construction invalid under ordinary struct visibility rules. Other zero-value
  creation paths are outside this proposal's field-visibility guarantee.
- Port must be `i32` (valid range 0–65535, validated at runtime).
- Host must be `str`.
- `max_bytes` for `read` must be 1 through 67,108,864 inclusive.
- `millis` for deadlines must be `i32` (non-negative, validated at runtime).
- All host-backed functions are errorable.

## 6. Grammar / Parsing Shape

No language grammar changes. The `net` package is imported with `import
"std/net"`; constructors and resolution are qualified functions, while resource
operations are methods on `Listener` and `Conn`.

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

- TCP helper implementations in `crates/yar-runtime`.
- Listener and connection state lives behind the validated runtime handle
  registry; runtime code never dereferences an `i64` as a native address.
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

`Addr`, `Conn`, and `Listener` are public named structs. `Addr` exposes its data;
the two resource types are package-owned and explicitly share-safe despite
containing internal registry tokens.

### Control flow

Networking calls block their calling native task thread. Sibling tasks continue,
and sibling close ends blocked accept/read/write with `error.Closed`.

### Builtins

No new builtins. `len`, `to_str`, etc. work on `net.Addr` fields naturally.

### Deadlines and synchronous host calls

Read and write deadlines are relative per-operation socket timeouts. Zero
disables a timeout, and changing one is not promised to interrupt a syscall
already running. DNS resolution and connect are synchronous host calls and
cannot be interrupted before a connection handle exists.

## 9. Alternatives Considered

### 1. Public raw handles

The original API exposed `i64` handles. It was superseded because raw scalars
bypassed resource typing and spawn-boundary ownership rules.

### 2. Combined "host:port" string addressing

Would match Go's `net.Dial("tcp", "host:port")` pattern. Rejected because
parsing "host:port" in C is fragile (IPv6 brackets, edge cases), and separate
parameters are more explicit, matching Yar's design philosophy.

### 3. Public non-blocking or multiplexed I/O

Would enable multiplexed servers without native task threads. Deferred because
the shipped runtime uses one native thread per task. The blocking public API is
implemented with internal nonblocking polling so close and operation-local
timeouts behave consistently across host platforms. Adaptive bounded waits
reduce idle wakeups, but each blocked call still consumes one native thread;
high-connection-count multiplexing remains deferred.

## 10. Complexity Cost

- **Language surface**: zero (no grammar changes)
- **Parser complexity**: zero
- **Checker complexity**: minimal (host-intrinsic metadata and stable error
  registration)
- **Codegen complexity**: moderate (14 new intrinsic cases, follows existing
  patterns exactly)
- **Runtime complexity**: moderate (Rust socket state, nonblocking polling,
  close coordination, and cross-platform behavior)
- **Diagnostics complexity**: zero
- **Test burden**: runtime ABI/unit tests plus concurrent native integration on
  Unix and Windows
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

Accepted. In the implemented baseline, TCP networking fills the most impactful gap in Yar's
stdlib with minimal compiler complexity, following established patterns.

## 14. Implementation Checklist

- [x] `stdlib/packages/net/net.yar` — Yar source declarations
- [x] `crates/yar-compiler/src/checker.rs` — host intrinsic registration, error
      names
- [x] `crates/yar-compiler/src/codegen.rs` — LLVM declarations, intrinsic codegen, error
      mapping
- [x] `crates/yar-runtime/src/net.rs` — Rust runtime implementation
- [x] `runtime-bundles/x86_64-pc-windows-gnu/yar-runtime.toml` and
      `crates/yar-cli/src/runtime_bundle.rs` — validated Windows native-library
      metadata including `ws2_32`
- [x] `testdata/stdlib_net/main.yar` — integration test fixture
- [x] Rust compiler and CLI tests — test function
- [x] `docs/context/domains/stdlib.md` — net package documentation
- [x] `docs/context/domains/error-model.md` — net error names
- [x] `docs/context/platform/toolchain-runtime.md` — net runtime surface
- [x] `docs/context/summary.md` — updated stdlib list
- [x] `docs/YAR.md` — net package reference
- [x] `docs/context/domains/language-slice.md` — updated stdlib list
