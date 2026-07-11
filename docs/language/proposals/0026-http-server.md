# Proposal: Minimal HTTP Server (`http` stdlib package)

Status: accepted and implemented

## 1. Summary

Add a pure-Yar `http` stdlib package for small HTTP/1.1 servers. The package
wraps the existing `net` TCP primitives, parses one request per connection,
calls a user handler, writes one response, and closes the connection.

This is a GTM-oriented capability: it lets Yar demonstrate a native network
service without adding an HTTP framework, auth system, router, TLS layer, or new
runtime intrinsic.

## 2. User-Facing API

```
import "std/http"
import "std/net"

pub struct Request {
    method str
    path str
    headers map[str]str
    body str
}

pub struct Response {
    status i32
    headers map[str]str
    body str
}

http.text(status i32, body str) http.Response
http.serve(addr net.Addr, handler fn(http.Request) !http.Response) !void
```

Example:

```yar
package main

import "std/http"
import "std/net"

fn handle(req http.Request) !http.Response {
    if req.path == "/health" {
        return http.text(200, "ok\n")
    }
    return http.text(200, "hello from Yar\n")
}

fn main() !i32 {
    http.serve(net.Addr{host: "127.0.0.1", port: 8080}, fn(req http.Request) !http.Response {
        return handle(req)
    })?
    return 0
}
```

## 3. Semantics

- `serve` calls `net.listen` and processes TCP connections sequentially.
- Each connection carries exactly one HTTP request and one HTTP response.
- The connection is closed after the response is written.
- Request header names are normalized to lowercase.
- `Content-Length` is honored for bodies up to 65536 bytes.
- Malformed requests receive `400 Bad Request`.
- Handler errors receive `500 Internal Server Error`; `serve` keeps accepting.
- `serve` returns only when listener setup or the accept loop fails.

## 4. Non-Goals

- no keep-alive
- no router
- no query parser
- no middleware
- no stdlib auth
- no sessions, JWT, or password handling
- no TLS
- no HTTP client
- no concurrent connection handling

## 5. Implementation Notes

The package is pure Yar and imports `conv`, `net`, `sort`, and `strings`.
Response headers are emitted in sorted order for deterministic output.

Top-level functions are not first-class values in the current language, so
callers pass a function literal when adapting a named handler to `serve`.
`serve` does not move that arbitrary handler value across a spawn boundary;
it finishes the current connection before accepting the next one.

## 6. Tests

- `testdata/stdlib_http/main.yar` checks the exported request/response API.
- `TestStdlibHTTPServeResponds` builds a temporary Yar server, runs it as a
  subprocess, sends a real TCP HTTP request, and validates the response.
- `examples/http_server/main.yar` is the user-facing sample app.
