# Proposal: HTTP Routing on the Bounded Server

Status: accepted

Implementation: implemented

## 1. Summary

Add method-aware path routing to `std/http`, built on the bounded server
connections from proposal 0026. Routes are declared as values, validated when
the router is built, and dispatched by `Router.serve`, which has the same shape
as a handler.

This replaces the withdrawn proposal 0027 without reviving its server
contract, request maps, or middleware scope.

## 2. Motivation

Handlers previously compared `req.method` and `req.target` by hand. That does
not scale past a few endpoints, does not decode paths, and gets `404`, `405`,
and `Allow` wrong by default.

## 3. Public API

```yar
pub error PathValueNotFound
pub error RouteConflict

pub struct Route { /* private */ }
pub struct Router { /* private */ }

pub fn route(method str, pattern str, handler fn(Request) !Response) !Route
pub fn router(routes []Route) !Router
pub fn (r Router) serve(req Request) !Response

pub struct Request {
    pub method str
    pub target str
    pub headers []Header
    pub body str
    pub path_values []url.Param
}
pub fn (r Request) path() str
pub fn (r Request) query() !url.Query
pub fn (r Request) path_value(name str) !str

pub fn json(status i32, body str) !Response
pub fn (r Response) status() i32
pub fn (r Response) body() str
pub fn (r Response) header(name str) !str
```

## 4. Example

```yar
fn get_user(req http.Request) !http.Response {
    return http.text(200, "user " + req.path_value("id")?)
}

router := http.router([]http.Route{
    http.route("GET", "/users/{id}", get_user)?,
    http.route("GET", "/files/{path...}", serve_file)?,
})?
connection.serve(fn(req http.Request) !http.Response {
    return router.serve(req)
})?
```

## 5. Patterns

- A pattern starts with `/` and is split into segments at `/`.
- `{name}` matches one non-empty segment. `{name...}` matches the rest of the
  path, including an empty rest after a trailing slash, and must be last.
- Other segments are literals made of URI path characters, without `%`,
  `{`, or `}`. An empty literal is allowed only as the last segment, which
  makes a trailing slash significant: `/users` and `/users/` differ.
- Parameter names are identifiers and must be unique in one pattern.
- Methods are HTTP tokens and match case-sensitively.
- Invalid methods or patterns return `http.InvalidArgument`.

## 6. Matching

1. `Request.path()` extracts the path from origin-form and absolute-form
   targets. Each raw segment is percent-decoded; a bad escape yields `400`.
2. Literals compare against decoded segments. Parameter values are decoded
   segments, so `%2F` inside a segment stays inside one value. A rest value is
   the decoded remainder.
3. Among routes for the request method, the most specific match wins:
   segments compare left to right, and a literal beats a parameter, which beats
   a rest segment.
4. `HEAD` uses a `HEAD` route when one matches, else the matching `GET` route.
5. If no route matches the method but some match the path, the router returns
   bodyless `405` with an `Allow` header in registration order, adding `HEAD`
   after `GET`. Otherwise it returns bodyless `404`.
6. The handler receives the request with `path_values` set.

`router` rejects two routes with the same method and the same segment shape
(the same kinds and literals, ignoring parameter names) with
`http.RouteConflict`. Because specificity is a total order on shapes, no two
accepted routes can tie for one request.

## 7. Interactions

- Named function values (proposal 0032) let route tables name handlers
  directly.
- `Router` holds function values, so it is not share-safe. Each worker task
  builds its own router, which is cheap and keeps handlers thread-local.
- Handler errors still become `500` through `Connection.serve`.
- `Response` accessors make handlers and routers testable without sockets.

## 8. Alternatives Considered

- Registration through a mutable router: Yar methods do not auto-address
  values, so a value-returning builder or a pointer API adds noise. A route
  slice is declarative and validates in one place.
- Go-style subtree matching for trailing slashes: explicit `{rest...}` is
  easier to read and never matches by accident.
- Middleware and extractors: out of scope; composition is plain functions.

## 9. Tests

- `testdata/stdlib_http_router`: literals, parameters, precedence, decoding,
  rest segments, absolute-form targets, queries, `HEAD` fallback, `404`,
  `405` with `Allow`, bad escapes, invalid patterns, and conflicts.
- `yar-cli` test `todo_api_example_serves_json_crud_over_http` exercises the
  router over real sockets.

## 10. Decision

Accepted. Routing is a thin, strict, allocation-light layer in `std/http`
that composes with the existing handler type.

## 11. Implementation Checklist

- [x] `stdlib/packages/http/router.yar`
- [x] request path, query, and path-value accessors
- [x] response accessors and `http.json`
- [x] fixtures, CLI test, and docs
