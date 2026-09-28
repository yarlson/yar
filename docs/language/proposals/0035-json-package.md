# Proposal: JSON Values (`json` stdlib package)

Status: accepted

Implementation: implemented

## 1. Summary

Add a pure-Yar `std/json` package with an explicit JSON value tree, a strict
RFC 8259 parser, a compact encoder, and small typed accessors.

## 2. Motivation

HTTP APIs exchange JSON. Yar has no reflection or struct tags, so a
serialization framework is not possible without new language machinery. An
explicit value tree fits the language: programs convert between their own
structs and `json.Value` with ordinary code.

## 3. Public API

```yar
pub enum Value {
    Null
    Bool { value bool }
    Number { text str }
    String { value str }
    Array { items []Value }
    Object { members []Member }
}

pub struct Member {
    pub name str
    pub value Value
}

pub fn parse(text str) !Value
pub fn encode(value Value) !str
pub fn int(value i64) Value
pub fn number(text str) !Value
pub fn get(value Value, name str) !Value
pub fn is_null(value Value) bool
pub fn as_bool(value Value) !bool
pub fn as_str(value Value) !str
pub fn as_i64(value Value) !i64
pub fn as_i32(value Value) !i32
pub fn as_array(value Value) ![]Value
pub fn as_object(value Value) ![]Member
```

Errors: `json.InvalidJSON`, `json.DuplicateName`, `json.TooDeep`,
`json.InvalidNumber`, `json.InvalidString`, `json.NotFound`,
`json.OutOfRange`, and `json.TypeMismatch`.

## 4. Example

```yar
body := json.parse(req.body)?
title := json.as_str(json.get(body, "title")?)?

reply := json.Value.Object([]json.Member{
    json.Member{name: "id", value: json.int(id)},
    json.Member{name: "title", value: json.Value.String(title)},
})
return http.json(201, json.encode(reply)?)
```

## 5. Semantics

- Numbers keep their validated source text. Yar has no floating-point type,
  and text keeps large integers and decimals exact. `as_i64` and `as_i32`
  accept only integer text (no fraction or exponent) in range, else
  `json.OutOfRange`.
- `parse` accepts exactly one value with surrounding JSON whitespace. It
  rejects invalid UTF-8, raw control characters, lone surrogate escapes,
  leading zeros, and trailing content with `json.InvalidJSON`. Duplicate
  object names return `json.DuplicateName`, following I-JSON, so lookups are
  never ambiguous. Nesting deeper than 128 arrays or objects returns
  `json.TooDeep`, which bounds recursion on untrusted input.
- `encode` writes compact JSON in member order. It escapes `"`, `\`, and
  control characters and writes other valid UTF-8 unchanged. Because enum
  payloads are public, it validates what callers construct: invalid number
  text returns `json.InvalidNumber`, invalid UTF-8 returns
  `json.InvalidString`, duplicate names return `json.DuplicateName`, and
  nesting over 128 (including cycles built through shared slices) returns
  `json.TooDeep`.
- `get` returns the member value for an object, `json.NotFound` for a missing
  name, and `json.TypeMismatch` for other kinds. Other accessors return
  `json.TypeMismatch` for the wrong kind.

## 6. Interactions

- Enums: `Value` is a public enum, so programs `match` on it directly and build
  values with positional constructors such as `json.Value.String("x")`.
- Concurrency: values containing slices are not share-safe; encode to `str`
  before sending across tasks.
- HTTP: `http.json(status, body)` sets `content-type: application/json`.

## 7. Alternatives Considered

- Reflection or derive-based struct mapping: needs language features Yar does
  not have and would hide control flow.
- `f64` numbers: Yar has no floating-point type; text is exact and lossless.
- Last-wins duplicate names: silently ambiguous for security-sensitive fields.

## 8. Tests

`testdata/stdlib_json` covers round trips, whitespace, every escape form,
surrogate pairs, invalid documents, duplicate names, the depth limit on both
sides, encoder validation, number construction, accessors, and range checks.

## 9. Decision

Accepted. JSON is an explicit value tree with strict parsing and validated
encoding.

## 10. Implementation Checklist

- [x] `stdlib/packages/json/json.yar` and loader registration
- [x] `testdata/stdlib_json`
- [x] `docs/YAR.md`, `LLM.txt`, and context docs
