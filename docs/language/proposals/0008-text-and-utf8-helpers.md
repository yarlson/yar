# Proposal: Text and UTF-8 Helpers

Status: accepted

## 1. Summary

Add helper functions for text-heavy programs, especially lexers and diagnostic
code, as standard library packages rather than builtins.

The helpers split across two stdlib packages:

**`strings` (additions to existing package):**

- `strings.parse_i64(str) !i64`
- `strings.from_byte(i32) str`

**`utf8` (new package):**

- `utf8.decode(str, i32) !i32`
- `utf8.width(str, i32) !i32`
- `utf8.is_letter(i32) bool`
- `utf8.is_digit(i32) bool`
- `utf8.is_space(i32) bool`

**`conv` (new package):**

- `conv.itoa(i32) str`
- `conv.itoa64(i64) str`

## 2. Motivation

Basic string operations are necessary but still not sufficient for a self-hosted
frontend.

The current Go frontend also depends on:

- UTF-8 decoding while scanning source
- letter, digit, and whitespace classification
- integer parsing for literals and array lengths
- integer-to-string conversion for diagnostics
- constructing a single-byte string from a byte value

These belong in the standard library, not as builtins. The stdlib infrastructure
now exists (`internal/stdlib/`) and the `strings` package is already shipped.
Adding new stdlib packages follows the established pattern.

## 3. User-Facing Examples

### Valid examples

```yar
package main

import "utf8"
import "strings"
import "conv"

fn advance(src str, offset i32) !i32 {
    w := utf8.width(src, offset)?
    return offset + w
}

fn starts_ident(src str, offset i32) !bool {
    r := utf8.decode(src, offset)?
    return utf8.is_letter(r)
}

fn parse_count(text str) !i64 {
    return strings.parse_i64(text)
}

fn format_line(line i32) str {
    return "line " + conv.itoa(line)
}

fn byte_to_str(b i32) str {
    return strings.from_byte(b)
}
```

### Invalid examples

```yar
ok := utf8.is_digit("7")
```

Invalid because classification helpers operate on decoded rune values as `i32`.

```yar
value := strings.parse_i64(10)
```

Invalid because `parse_i64` requires a string.

```yar
width := utf8.width(src, true)
```

Invalid because UTF-8 helpers require an integer byte offset.

## 4. Semantics

These helpers are ordinary functions in stdlib packages. They do not add hidden
control flow beyond the existing `!T` error model.

- `utf8.decode(s, off)` decodes the rune beginning at byte offset `off`
- `utf8.width(s, off)` returns the byte width of the rune beginning at byte
  offset `off`
- `utf8.is_letter`, `utf8.is_digit`, and `utf8.is_space` classify a decoded
  rune value
- `strings.parse_i64` parses a base-10 signed integer
- `strings.from_byte` constructs a single-byte string from a byte value
- `conv.itoa` and `conv.itoa64` produce base-10 decimal strings

UTF-8 and parse failures are explicit errors:

- `error.InvalidUTF8`
- `error.OutOfRange`
- `error.InvalidInteger`
- `error.IntegerOverflow`

## 5. Type Rules

- `utf8.decode(str, i32) !i32`
- `utf8.width(str, i32) !i32`
- `utf8.is_letter(i32) bool`
- `utf8.is_digit(i32) bool`
- `utf8.is_space(i32) bool`
- `strings.parse_i64(str) !i64`
- `strings.from_byte(i32) str`
- `conv.itoa(i32) str`
- `conv.itoa64(i64) str`

These are ordinary stdlib functions. The checker and codegen treat them like any
other imported package function — no special handling needed.

## 6. Grammar / Parsing Shape

No new grammar is required. These are standard library functions imported and
called through the existing package-qualified syntax.

## 7. Lowering / Implementation Model

### What can be written in pure yar

With proposal 0006 primitives (`len`, `s[i]`, `s[i:j]`, `+`, `==`):

- `utf8.is_letter(i32) bool` — range checks on rune value
- `utf8.is_digit(i32) bool` — range check `48..57`
- `utf8.is_space(i32) bool` — check against known whitespace codepoints
- `utf8.decode(str, i32) !i32` — manual UTF-8 byte decoding
- `utf8.width(str, i32) !i32` — leading byte inspection
- `strings.parse_i64(str) !i64` — digit-by-digit accumulation
- `strings.from_byte(i32) str` — needs a new builtin (see below)

### What needs new builtins or runtime support

- `strings.from_byte(i32) str`: requires constructing a `%yar.str` from a
  single byte value. This cannot be expressed in pure yar today. Options:
  - Add a minimal `chr(i32) str` builtin that creates a one-byte string
  - Add a runtime helper `yar_str_from_byte(i32) %yar.str`

- `conv.itoa(i32) str` and `conv.itoa64(i64) str`: technically implementable
  in pure yar using digit extraction and `strings.from_byte`, but awkward
  without a string builder. Options:
  - Implement in yar once `from_byte` exists (correct but O(n^2) from concat)
  - Add runtime helpers for efficiency

### Recommended approach

Add one small builtin or runtime helper — `chr(i32) str` — that constructs a
one-byte string. Everything else can be written in pure yar on top of it.

- `internal/stdlib/packages/utf8/utf8.yar` — all five utf8 functions in yar
- `internal/stdlib/packages/strings/strings.yar` — add `parse_i64` and
  `from_byte` (wrapping `chr`)
- `internal/stdlib/packages/conv/conv.yar` — `itoa` and `itoa64` in yar using
  `from_byte` for digit characters

### Implementation steps

- parser: no changes
- AST / IR: no new node kinds
- checker: register `chr` builtin if chosen
- codegen: lower `chr` to runtime call
- runtime: add `yar_str_from_byte(i32) %yar.str`
- stdlib: write `utf8.yar`, extend `strings.yar`, write `conv.yar`
- tests: stdlib integration tests for each package

## 8. Interactions

- errors: helper failures compose with `?` and `or |err| { ... }`
- structs: helper results fit naturally in lexer/parser state structs
- stdlib: extends the existing stdlib infrastructure with new packages
- builtins: adds at most one new builtin (`chr`), keeping the builtin surface
  small
- future richer type features: later char, rune, or byte types can refine this
  surface without invalidating the core need

## 9. Alternatives Considered

### Leave all text utilities to user code

Rejected because some primitives, especially UTF-8 decoding, are too fundamental
to expect every early program to reimplement.

### Add all helpers as builtins

Rejected. The stdlib infrastructure now exists and these functions do not need
compiler-level special handling. Keeping the builtin surface small is an explicit
design goal.

### Add a large text standard library immediately

Rejected because the first need is a small self-hosting-oriented helper set, not
an expansive library design.

### Add only ASCII helpers

Rejected because the current frontend already relies on UTF-8-aware scanning,
and self-hosting should not force an immediate capability regression.

## 10. Complexity Cost

- language surface: low (at most one new builtin)
- parser complexity: none
- checker complexity: low (one builtin registration if `chr` is added)
- lowering/codegen complexity: low (one runtime call if `chr` is added)
- runtime complexity: low
- stdlib complexity: medium (three packages with several functions)
- diagnostics complexity: low
- test burden: medium
- documentation burden: medium

## 11. Why Now?

Without these helpers, a self-hosted lexer either loses current UTF-8 behavior
or must rely on ad hoc runtime support that is never documented as part of the
language surface. The stdlib infrastructure is in place and proven with the
`strings` package.

## 12. Open Questions

- Should `chr` be a builtin or a runtime-only helper accessed through stdlib?
- Should YAR later grow a dedicated `rune` or `byte` type instead of using `i32`
  here?
- Should `utf8.decode` and `utf8.width` eventually merge into a single richer
  decode result once the language has a better small-product type story?
- Should `conv` be a separate package or should `itoa`/`itoa64` live in
  `strings`?

## 13. Decision

Accepted.

Implemented with three builtins (`chr`, `i32_to_i64`, `i64_to_i32`) instead of
the originally proposed single `chr` builtin. The additional integer conversion
builtins were needed because the language lacks implicit widening/narrowing
between `i32` and `i64`, making pure-yar implementations of `parse_i64` and
`itoa64` impossible without them.

This proposal deliberately keeps the helper set small and focused on the needs
of lexing, parsing, and diagnostics, while leveraging the stdlib infrastructure
rather than growing the builtin surface.

## 14. Implementation Checklist

- `chr` builtin or runtime helper
- `internal/stdlib/packages/utf8/utf8.yar`
- `internal/stdlib/packages/strings/strings.yar` additions (`parse_i64`,
  `from_byte`)
- `internal/stdlib/packages/conv/conv.yar`
- integration tests for each stdlib package
- `docs/YAR.md` update
- `docs/context/` update
- `decisions.md` update
