# Proposal: Bounded HTTP/1.1 Server (`http` stdlib package)

Status: accepted
Implementation: implemented

## 1. Summary

Add a pure-Yar `http` standard-library package for bounded HTTP/1.1 server
connections.

The package uses the existing typed `net.Listener` and `net.Conn` resources.
It owns HTTP framing, validation, response serialization, deadlines, and
per-connection cleanup. The caller owns the accept loop, concurrency,
cancellation, logging, and error policy.

The accepted design replaces the withdrawn whole-request experiment. It is:

- incremental across arbitrary TCP read boundaries
- bounded by explicit head and body limits
- strict about request and header grammar
- unambiguous about message framing
- safe against response splitting
- explicit about connection ownership and errors

## 2. Motivation

Yar has the substrate needed for a safe small HTTP server: typed TCP resources,
bounded reads, operation deadlines, explicit close, structured concurrency, and
ordinary errors as values. The removed `std/http` package did not use that
substrate correctly. It assumed the request head arrived in one read, accepted
ambiguous framing, emitted unvalidated response headers, hid connection errors,
and lacked a real socket test.

HTTP is useful only when malformed and adversarial traffic is part of the
design. A compile-only demonstration is not sufficient for a standard-library
protocol boundary.

## 3. Public API

```yar
import "std/http"
import "std/net"

pub struct Header {
    pub name str
    pub value str
}

pub struct Request {
    pub method str
    pub target str
    pub headers []Header
    pub body str
}

pub struct Limits {
    // package-owned fields
}

pub struct Response {
    // package-owned fields
}

pub struct Server {
    // package-owned listener and limits
}

pub struct Connection {
    // package-owned connection and limits
}

pub fn default_limits() Limits
pub fn limits(
    max_head_bytes i32,
    max_body_bytes i32,
    read_timeout_millis i32,
    write_timeout_millis i32,
) !Limits

pub fn listen(addr net.Addr, limits Limits) !Server
pub fn response(status i32, body str) !Response
pub fn text(status i32, body str) !Response

pub fn (r Request) header(name str) !str
pub fn (r Request) header_values(name str) ![]str
pub fn (r Response) with_header(name str, value str) !Response
pub fn (r Response) add_header(name str, value str) !Response

pub fn (s Server) accept() !Connection
pub fn (s Server) addr() !net.Addr
pub fn (s Server) close() !void

pub fn (c Connection) serve(handler fn(Request) !Response) !void
pub fn (c Connection) local_addr() !net.Addr
pub fn (c Connection) remote_addr() !net.Addr
pub fn (c Connection) close() !void
```

`Limits`, `Response`, `Server`, and `Connection` have private fields. External
code cannot fabricate unchecked limits, responses, or resource handles.

## 4. Example

```yar
package main

import "std/http"
import "std/net"

fn handle(req http.Request) !http.Response {
    if req.method == "GET" && req.target == "/health" {
        return http.text(200, "ok\n")
    }
    return http.text(404, "not found\n")
}

fn main() !i32 {
    server := http.listen(
        net.Addr{host: "127.0.0.1", port: 8080},
        http.default_limits(),
    )?

    for true {
        connection := server.accept()?
        connection.serve(fn(req http.Request) !http.Response {
            return handle(req)
        }) or |err| {
            print("http connection failed: " + to_str(err) + "\n")
        }
    }
    return 0
}
```

The explicit loop is intentional. `http` does not hide an unbounded task set,
swallow connection failures, or choose an application logging policy.

## 5. Limits And Lifetimes

`default_limits()` returns:

- 32 KiB maximum request or response head, including the terminating empty line
- 1 MiB maximum decoded request body
- 5 second total request-read deadline
- 5 second total response-write deadline

`limits(...)` accepts:

- `max_head_bytes`: 1024 through 1048576, applied independently to request and
  response heads
- `max_body_bytes`: 0 through 67108864
- read and write timeouts: 1 through 3600000 milliseconds

`Connection.serve` installs fixed deadlines, measured from the setter calls,
before reading and writing. Unlike `net.Conn`'s existing per-operation timeout,
the fixed read deadline does not reset when a fragmented request produces
another successful read. It processes exactly one request and writes exactly
one final response. The connection is closed on
success, protocol failure, handler failure, and transport failure. Before close,
it shuts down the write half so the response and EOF are sent, then drains at
most 64 KiB for at most 100 milliseconds so unread queued input cannot turn an
otherwise complete response into a reset. The returned
error retains the original protocol, handler, or `net` identity when cleanup
also fails.

`Connection.close` exists for callers that accept a connection but decide not
to serve it. `Server.close` closes the listener and wakes a blocked accept
according to the existing `net.Listener` contract.

## 6. Request Parsing

The server accepts HTTP/1.1 requests only.

- Reads are incremental; delimiters may cross TCP read boundaries.
- Request heads require CRLF line endings and are capped before parsing.
- A target that fills the head cap before its terminating space receives `414`;
  a completed request line followed by an oversized header section receives
  `431`. Other incomplete overlong start lines are bad requests.
- The request line has exactly `method SP request-target SP HTTP/1.1`.
- Malformed HTTP-version grammar is a bad request; a syntactically valid version
  other than HTTP/1.1 receives `505`.
- Methods and field names use the HTTP token grammar.
- Origin-form, absolute-form, and `OPTIONS *` request targets are validated as
  URI syntax. Fragments, authority-form outside `CONNECT`, and `*` for other
  methods are rejected. `CONNECT` itself remains unsupported.
- Header field names are normalized to lowercase.
- Field values reject control bytes other than horizontal tab.
- Obsolete folded fields are rejected.
- Exactly one syntactically valid, non-empty `Host` authority is required. For
  absolute-form targets, the target authority replaces the received `Host`
  value before the handler runs.
- Duplicate `Content-Length` is rejected, even when values agree.
- A request containing both `Content-Length` and `Transfer-Encoding` is
  rejected and closed.
- `Content-Length` is strict unsigned decimal and cannot exceed the body cap.
- `Transfer-Encoding` supports exactly one final `chunked` coding. Other or
  repeated codings receive `501 Not Implemented`.
- Chunk sizes are strict hexadecimal. Chunk-extension names and values follow
  token/quoted-string grammar. Chunk metadata and trailers are bounded by the
  request-head cap. Trailer syntax is validated and framing fields in trailers
  are rejected. Trailer values are otherwise discarded.
- `Expect` supports `100-continue`; other expectations receive
  `417 Expectation Failed`.
- `CONNECT` receives `501 Not Implemented`; this package does not create
  tunnels.
- Bytes already read after the selected request body are discarded when the
  connection closes, so pipelined input cannot become another request.

Protocol failures receive a bounded, bodyless response before close when the transport
still permits it. After writing that response, the package drains at most 64
KiB for at most 100 milliseconds before close. This improves reliable delivery
when malformed input is already queued without allowing unbounded cleanup.
Protocol failures are also returned as package-owned errors so the caller can
observe and log them.

## 7. Response Model

`response(status, body)` accepts final status codes from 200 through 599.
Status codes that prohibit content reject a non-empty body. `text` additionally
sets `content-type: text/plain; charset=utf-8`.

`with_header` validates the field name and value, normalizes the name to
lowercase, and returns an independently backed response with that field added or
all existing instances replaced. `add_header` preserves existing instances and
appends another field, which supports fields such as `Set-Cookie`. Applications
cannot set connection/framing fields owned by the package:

- `connection`
- `content-length`
- `trailer`
- `transfer-encoding`
- `upgrade`

Serialization always uses `HTTP/1.1`, a valid status line with an empty reason
phrase, an explicit `Content-Length` where the status permits it, and
`Connection: close`. Header names and values are
revalidated at serialization as a defensive invariant. No application value
can inject CR or LF into the response head.

Responses to `HEAD` omit body bytes while reporting the length of the body that
would have been sent. The handler owns keeping that metadata aligned with the
corresponding `GET`. Statuses 204, 205, and 304 do not send body bytes.

## 8. Errors

The package declares public errors for protocol and API failures:

- `http.BadRequest`
- `http.BodyTooLarge`
- `http.ExpectationFailed`
- `http.HeaderNotFound`
- `http.HeaderTooLarge`
- `http.HTTPVersionNotSupported`
- `http.InvalidArgument`
- `http.InvalidResponse`
- `http.URITooLong`
- `http.UnsupportedMethod`
- `http.UnsupportedTransferEncoding`

Transport errors keep their `net` or `error.Closed` identity. Handler errors
keep the handler's original package-owned identity after the package attempts a
minimal `500 Internal Server Error` response.

## 9. Runtime And Compiler Boundary

HTTP parsing and policy are ordinary Yar code over `std/net` and existing
string, slice, map, and integer operations. The bounded exchange requires three
small additions to the existing `net.Conn` host boundary:

```yar
pub fn (c Conn) set_read_deadline_after(millis i32) !void
pub fn (c Conn) set_write_deadline_after(millis i32) !void
pub fn (c Conn) shutdown_write() !void
```

Each setter stores one fixed deadline relative to the time of the call. Zero
disables it. Existing `set_read_deadline` and `set_write_deadline` retain their
per-operation timeout behavior.
`shutdown_write` serializes after queued writes, sends EOF, and preserves the
read half for bounded cleanup before close.

One-shot string construction also requires consuming forms of the existing
builder substrate:

```yar
sb_finish(builder i64) str
sb_discard(builder i64) void
```

`sb_finish` extracts the value and releases the registry handle. `sb_discard`
releases a partial builder on an error path. Existing `sb_string` retains its
reset-and-reuse behavior. These operations prevent per-request builder-handle
growth in string normalization and chunk decoding.

The design adds:

- no syntax
- two lifecycle operations to the existing string-builder builtin family
- no dependency

The compiler and runtime additions project the two fixed deadline setters
through the existing networking intrinsic pattern and add consuming operations
to the existing builder ABI. HTTP grammar, framing, and serialization do not
move into the runtime.

The runtime continues to own TCP handles, blocking operations, deadlines, and
close coordination. The `http` package owns protocol policy and serialization.

## 10. Tests

Executable native socket tests cover:

- a request head fragmented across writes
- a `Content-Length` body split across reads
- a fragmented chunked body and terminating trailers
- exact accepted head and body limits
- duplicate and conflicting framing headers
- malformed request lines and header fields
- missing and duplicate `Host`
- unsupported transfer codings and expectations
- read deadline expiry
- response-header injection rejection
- response-head limit enforcement
- binary request/response bodies, including NUL bytes
- absolute-form authority normalization and request-target forms
- valid and malformed chunk extensions
- `100-continue`
- body-forbidden statuses and bodyless automatic errors for `HEAD`
- handler-error identity and connection cleanup

Compiler tests also prove that private resource/config/response fields cannot be
constructed or selected outside `std/http`.

## 11. Non-Goals

- HTTP client
- HTTP/2 or HTTP/3
- TLS
- routing or middleware
- keep-alive or pipelining
- request or response streaming APIs
- compression
- WebSocket or protocol upgrades
- CONNECT tunnels
- automatic background tasks
- application logging, metrics, or panic recovery

These require separate pressure, contracts, and proposals. They are not hidden
extension points in this package.

## 12. Standards Basis

Framing and grammar follow RFC 9112. Semantics and status/body rules follow RFC
9110. Where the specifications permit leniency, the package chooses one strict
interpretation to avoid differential parsing and request-smuggling behavior.

## 13. Decision

Accepted. Restore `std/http` as an explicit, bounded HTTP/1.1 server-connection
package over the existing streaming resource model. Do not restore the removed
single-read `serve(addr, handler)` API.

## 14. Implementation Checklist

- [x] `stdlib/packages/http/{http,request,response,grammar}.yar`
- [x] compiler stdlib embedding and package tests
- [x] executable native socket fixture
- [x] adversarial CLI integration tests
- [x] `docs/context` current-state updates
- [x] `docs/YAR.md` public API update
- [x] `LLM.txt` compact mirror update
- [x] `README.md` capability and example update
- [x] proposal registry, decisions, and roadmap synchronization
