# Proposal: HTTP Routing (`http` stdlib package)

Status: draft

## 1. Summary

Add a small route matcher to the existing pure-Yar `http` stdlib package.
Routing stays inside `http` rather than becoming a separate framework package.

The router lets small native services register method-aware path patterns,
extract path parameters, parse query strings, and get correct `404 Not Found`
and `405 Method Not Allowed` responses without hand-written dispatch chains.

This proposal extends the minimal HTTP server from proposal 0026. It does not
add middleware, JSON, auth, static files, TLS, keep-alive, request extractors, or
a general web framework.

## 2. Prior Art

The design borrows only the parts that fit Yar's current language and stdlib
shape:

- Go 1.22 `net/http.ServeMux`: method-aware patterns, wildcards, deterministic
  specificity, and registration-time conflict detection.
- `go-chi/chi`: small router surface, named path parameters, and compatibility
  with a plain request/response handler model.
- Gin: ergonomic method helpers such as `GET` and `POST`, without adopting its
  framework context or middleware model.
- Rust `axum` and Actix Web: clear separation between route path parameters,
  query data, and request bodies, without copying typed extractor machinery that
  Yar cannot express cleanly today.
- Zig `http.zig`: explicit method helpers, named params on the request, and
  suffix catch-all routes, while avoiding ambiguous matching that requires
  request-time backtracking.

## 3. User-Facing API

The router lives in `http`.

```yar
import "http"
import "net"

pub struct Request {
    method str
    target str
    path str
    params map[str]str
    headers map[str]str
    body str
}

pub struct Response {
    status i32
    headers map[str]str
    body str
}

pub struct Query {
    values map[str][]str
}

pub struct Route {
    method str
    pattern str
    handler fn(Request) !Response
}

pub struct Router {
    routes []Route
}

pub fn router() Router

pub fn route(r *Router, method str, pattern str, handler fn(Request) !Response) !void
pub fn get(r *Router, pattern str, handler fn(Request) !Response) !void
pub fn post(r *Router, pattern str, handler fn(Request) !Response) !void
pub fn put(r *Router, pattern str, handler fn(Request) !Response) !void
pub fn patch(r *Router, pattern str, handler fn(Request) !Response) !void
pub fn del(r *Router, pattern str, handler fn(Request) !Response) !void

pub fn handle(r *Router, req Request) !Response
pub fn serve_router(addr net.Addr, r *Router) !void

pub fn param(req Request, name str) !str
pub fn query(req Request) Query
```

The existing `text` and `serve` functions remain available. `serve_router`
adapts a `Router` to the existing server loop.

Example:

```yar
package main

import "http"
import "net"

fn show_user(req http.Request) !http.Response {
    id := http.param(req, "id")?
    return http.text(200, "user " + id + "\n")
}

fn main() !i32 {
    r := http.router()

    http.get(&r, "/health", fn(req http.Request) !http.Response {
        return http.text(200, "ok\n")
    })?

    http.get(&r, "/users/{id}", fn(req http.Request) !http.Response {
        return show_user(req)
    })?

    http.serve_router(net.Addr{host: "127.0.0.1", port: 8080}, &r)?
    return 0
}
```

## 4. Request Target and Query Semantics

`Request.target` contains the raw HTTP request target from the request line, for
example `/users/42?tab=posts`.

`Request.path` contains only the path used for route matching, for example
`/users/42`.

The query string is never part of route matching.

`query(req)` parses the query string from `target` and returns repeated keys as
multiple values:

```yar
q := http.query(req)
values := q.values["tag"]?
```

The v1 query parser splits on `&` and `=`. Percent-decoding is not part of v1
unless a separate string/URL decoding helper exists first.

## 5. Pattern Syntax

Patterns are UTF-8 strings matched by byte segment. All patterns must start with
`/`.

Supported forms:

- `/users` matches exactly `/users`
- `/users/{id}` captures one segment into param `id`
- `/users/{id}/posts` captures one segment in the middle of the path
- `/files/{path...}` captures the remaining suffix into param `path`

Rules:

- static segments match themselves
- `{name}` matches one non-empty path segment
- `{name...}` matches the remaining suffix and must be the final segment
- parameter names must be non-empty identifiers
- the same parameter name cannot appear twice in one pattern
- trailing slash is explicit: `/users` and `/users/` are different routes
- query strings are rejected in route patterns

Regular-expression constraints are intentionally omitted. They create a second
matching language and push validation into the router instead of handler code.

## 6. Method Semantics

Route methods are exact strings. The method helpers register uppercase standard
methods:

- `get` registers `GET`
- `post` registers `POST`
- `put` registers `PUT`
- `patch` registers `PATCH`
- `del` registers `DELETE`

The helper is named `del` because `delete` is already a global Yar builtin for
map deletion and cannot be redeclared as an `http` package function.

`route` registers any non-empty uppercase method string. Lowercase or empty
method strings return `error.InvalidRoute`.

`HEAD` and `OPTIONS` are not special in v1. Programs can register them through
`route` if needed.

## 7. Matching Semantics

The router picks exactly one route or returns a generated response.

For a request:

1. Match by path shape without considering method.
2. If no path shape matches, return `404 Not Found`.
3. If one or more path shapes match but none has the request method, return
   `405 Method Not Allowed`.
4. If a route matches both path and method, call its handler with `params`
   populated.

Specificity order:

1. static segment
2. `{name}` parameter segment
3. `{name...}` catch-all suffix

Longer static matches are more specific than shorter catch-all matches. When two
registered patterns could match the same request path with the same specificity,
registration returns `error.InvalidRoute` instead of leaving dispatch ambiguous.

Examples:

- `/users/me` beats `/users/{id}`
- `/files/static` beats `/files/{path...}`
- `/files/{path...}` matches `/files/a/b/c`
- `/users/{id}` and `/users/{name}` conflict for the same method

The implementation should reject ambiguous route sets during registration. It
should not use request-time backtracking to guess which route the author meant.

## 8. Handler and Error Contract

Route handlers keep the existing HTTP server contract:

```yar
fn(Request) !Response
```

If a matched handler returns an error, the router lets that error propagate to
`serve_router`. `serve_router` uses the existing server behavior: handler errors
become `500 Internal Server Error`, the connection closes, and the server keeps
accepting.

`handle(r, req)` is useful for tests and for programs that want to adapt routing
without opening a socket. It does not hide handler errors.

## 9. Generated Responses

The router generates plain text responses for unmatched requests:

- `404 Not Found` with body `not found\n`
- `405 Method Not Allowed` with body `method not allowed\n`

The response writer still sets `content-length` and a default text content type
when the response does not provide them.

Custom not-found or method-not-allowed handlers are not part of v1. They can be
added later without changing route matching.

## 10. Implementation Notes

The router should be pure Yar code in `stdlib/packages/http/http.yar`.

The simplest acceptable implementation stores routes in registration order and
performs a linear scan in `handle`. That is enough for the current stdlib scale
and keeps the matching rules easy to verify. A trie can be introduced later if
real programs show that route count or dispatch cost matters.

Route registration should parse and validate each pattern once, then store the
original method, original pattern, and handler. If parsed route metadata becomes
too awkward to represent in Yar v1, the implementation may re-parse patterns in
`handle`, but validation must still happen during registration.

## 11. Tests

Test coverage should include:

- route registration helpers for standard methods
- exact static matches
- parameter extraction
- catch-all suffix extraction
- query splitting independent from route matching
- static routes beating parameter routes
- parameter routes beating catch-all routes
- `404` when no path matches
- `405` when the path matches but method does not
- duplicate route rejection
- ambiguous route rejection
- invalid pattern rejection
- `serve_router` integration through a real TCP request

The existing HTTP subprocess test pattern from proposal 0026 should be reused
for the socket-level integration test. Pure `handle` tests should cover most of
the route matrix without starting long-running servers.

## 12. Non-Goals

- no separate `http/router` package
- no middleware
- no route groups
- no nested routers
- no mounted sub-apps
- no typed request extractors
- no JSON response helper
- no body decoding
- no regex route constraints
- no percent-decoding in v1
- no static file serving
- no auth, sessions, JWT, or cookies
- no TLS
- no keep-alive
- no HTTP client
