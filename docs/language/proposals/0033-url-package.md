# Proposal: URL Encoding and Query Parsing (`url` stdlib package)

Status: accepted

Implementation: implemented

## 1. Summary

Add a pure-Yar `std/url` package for percent-encoding, percent-decoding, and
`application/x-www-form-urlencoded` query parsing.

## 2. Motivation

HTTP handlers need decoded path segments, query parameters, and form bodies.
The bounded server from proposal 0026 validates request targets but hands
handlers the raw target. Without a shared decoder, every program and the router
would carry its own escape handling.

## 3. Public API

```yar
pub error InvalidEscape
pub error NotFound

pub struct Param {
    pub name str
    pub value str
}

pub struct Query { /* private ordered parameters */ }

pub fn percent_decode(s str) !str
pub fn percent_encode(s str) str
pub fn parse_query(raw str) !Query
pub fn (q Query) get(name str) !str
pub fn (q Query) values(name str) []str
pub fn (q Query) params() []Param
```

## 4. Semantics

- `percent_decode` replaces each `%XX` escape with its byte. `+` stays `+`.
  A truncated or non-hex escape returns `url.InvalidEscape`. Decoded bytes are
  not required to be UTF-8.
- `percent_encode` keeps RFC 3986 unreserved bytes (`A-Z a-z 0-9 - . _ ~`) and
  writes every other byte as uppercase `%XX`. The result is safe in a path
  segment, a query name, or a query value.
- `parse_query` splits on `&`, skips empty pairs, splits each pair at the first
  `=`, turns `+` into a space, and percent-decodes names and values. A pair
  without `=` has an empty value. Order and repeated names are preserved.
- `Query.get` returns the first value or `url.NotFound`; `values` returns all
  values in order; `params` returns an independent copy of every pair.

## 5. Example

```yar
query := url.parse_query("q=native+code&page=2")?
term := query.get("q")?          // "native code"
page := strings.parse_i64(query.get("page")?)?
```

## 6. Interactions

`std/http` uses the package for `Request.query()` and router path decoding.
`Query` has private fields, so callers obtain it only through `parse_query`.

## 7. Alternatives Considered

- A full URL type with scheme, host, and user info: the server receives
  request targets that `std/http` already validates, and no current program
  needs client-side URL composition.
- Map-based queries: maps lose order and repeated names that forms and APIs
  depend on.
- Treating `;` as a separator: modern servers reject that legacy form; `;` is
  an ordinary byte here.

## 8. Tests

`testdata/stdlib_url` covers decoding, invalid escapes, encoding of reserved
and non-ASCII bytes, repeated names, flags without `=`, `+` handling, empty
queries, and invalid query escapes.

## 9. Decision

Accepted as a small pure-Yar package with ordered query parameters.

## 10. Implementation Checklist

- [x] `stdlib/packages/url/url.yar` and loader registration
- [x] `testdata/stdlib_url`
- [x] `docs/YAR.md`, `LLM.txt`, and context docs
