# Proposal: Text and UTF-8 Helpers

Status: proposed

## 1. Summary

Add a small helper surface for text-heavy programs, especially lexers and
diagnostic code.

The initial helper set is:

- `utf8_rune(str, i32) !i32`
- `utf8_width(str, i32) !i32`
- `is_letter(i32) bool`
- `is_digit(i32) bool`
- `is_space(i32) bool`
- `parse_i64(str) !i64`
- `itoa(i32) str`
- `itoa64(i64) str`

## 2. Motivation

Basic string operations are necessary but still not sufficient for a self-hosted
frontend.

The current Go frontend also depends on:

- UTF-8 decoding while scanning source
- letter, digit, and whitespace classification
- integer parsing for literals and array lengths
- integer-to-string conversion for diagnostics

Some of these could eventually live in a standard library. The immediate need is
to define a minimal and portable surface somewhere in the language ecosystem.

## 3. User-Facing Examples

### Valid examples

```yar
fn advance(src str, offset i32) !i32 {
    width := utf8_width(src, offset)?
    return offset + width
}

fn starts_ident(src str, offset i32) !bool {
    r := utf8_rune(src, offset)?
    return is_letter(r)
}

fn parse_count(text str) !i64 {
    return parse_i64(text)
}

fn format_line(line i32) str {
    return "line " + itoa(line)
}
```

### Invalid examples

```yar
ok := is_digit("7")
```

Invalid because classification helpers operate on decoded rune values as `i32`.

```yar
value := parse_i64(10)
```

Invalid because `parse_i64` requires a string.

```yar
width := utf8_width(src, true)
```

Invalid because UTF-8 helpers require an integer byte offset.

## 4. Semantics

These helpers are ordinary explicit operations. They do not add hidden control
flow beyond the existing `!T` error model.

- `utf8_rune(s, off)` decodes the rune beginning at byte offset `off`
- `utf8_width(s, off)` returns the byte width of the rune beginning at byte
  offset `off`
- `is_letter`, `is_digit`, and `is_space` classify a decoded rune value
- `parse_i64` parses a base-10 signed integer
- `itoa` and `itoa64` produce base-10 decimal strings

UTF-8 and parse failures are explicit. The initial builtin error set should
include at least:

- `error.InvalidUTF8`
- `error.OutOfRange`
- `error.InvalidInteger`
- `error.IntegerOverflow`

## 5. Type Rules

- `utf8_rune(str, i32) !i32`
- `utf8_width(str, i32) !i32`
- `is_letter(i32) bool`
- `is_digit(i32) bool`
- `is_space(i32) bool`
- `parse_i64(str) !i64`
- `itoa(i32) str`
- `itoa64(i64) str`

These helpers are compiler-owned contracts in the same way as current builtins.

## 6. Grammar / Parsing Shape

No new grammar is required if these are introduced as builtins or standard
library functions with fixed signatures.

## 7. Lowering / Implementation Model

- parser: no syntax changes required
- AST / IR: no new node kinds beyond ordinary calls
- checker: register the helper signatures and validate argument types
- codegen: lower calls to runtime helper functions
- runtime: implement UTF-8 decode, classification, integer parse, and integer
  formatting support

This proposal is intentionally light on syntax and heavy on library/runtime
support.

## 8. Interactions

- errors: helper failures compose with `?` and `or |err| { ... }`
- structs: helper results fit naturally in lexer/parser state structs
- arrays: no special interaction
- control flow: enables branch conditions based on decoded text classes
- returns: parse and decode results can propagate directly
- builtins: extends the compiler-owned builtin surface in a focused way
- future modules/imports: these helpers could later move behind an imported core
  package without changing semantics
- future richer type features: later char, rune, or byte types can refine this
  surface without invalidating the core need

## 9. Alternatives Considered

### Leave all text utilities to user code

Rejected because some primitives, especially UTF-8 decoding, are too fundamental
to expect every early program to reimplement.

### Add a large text standard library immediately

Rejected because the first need is a small self-hosting-oriented helper set, not
an expansive library design.

### Add only ASCII helpers

Rejected because the current frontend already relies on UTF-8-aware scanning,
and self-hosting should not force an immediate capability regression.

## 10. Complexity Cost

- language surface: low
- parser complexity: low
- checker complexity: low to medium
- lowering/codegen complexity: low to medium
- runtime complexity: medium
- diagnostics complexity: low
- test burden: medium
- documentation burden: medium

## 11. Why Now?

Without these helpers, a self-hosted lexer either loses current UTF-8 behavior
or must rely on ad hoc runtime support that is never documented as part of the
language surface.

## 12. Open Questions

- Should these helpers start as builtins or as a special imported core package
  once imports exist?
- Should YAR later grow a dedicated `rune` or `byte` type instead of using `i32`
  here?
- Should `utf8_rune` and `utf8_width` eventually merge into a single richer
  decode result once the language has a better small-product type story?

## 13. Decision

Pending.

This proposal deliberately keeps the helper set small and focused on the needs
of lexing, parsing, and diagnostics.

## 14. Implementation Checklist

- checker builtin registration
- runtime helper functions
- codegen call lowering
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
