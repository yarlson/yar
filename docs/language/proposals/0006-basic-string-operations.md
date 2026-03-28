# Proposal: Basic String Operations

Status: accepted

## 1. Summary

Add the minimum string operations needed for practical text processing:

- `len(str) i32`
- string `==` and `!=`
- string concatenation with `+`
- byte indexing `s[i]`
- byte slicing `s[i:j]`

## 2. Motivation

Current YAR treats `str` mostly as an opaque printable value.

That is enough for tiny programs, but not enough for compiler work. A
self-hosted lexer and parser need to:

- measure source text length
- compare identifiers against keywords
- inspect source one byte at a time
- slice substrings for tokens and diagnostics
- build readable diagnostic strings

Without these operations, `str` is too limited for frontend self-hosting.

## 3. User-Facing Examples

### Valid examples

```yar
fn is_main(name str) bool {
    return name == "main"
}

fn first_byte(src str) i32 {
    return src[0]
}

fn prefix(src str, n i32) str {
    return src[0:n]
}

fn describe(name str) str {
    return "unexpected token: " + name
}
```

### Invalid examples

```yar
ok := "a" == 1
```

Invalid because string equality requires two `str` operands.

```yar
x := "abc"[true]
```

Invalid because string indexing requires an integer index.

```yar
msg := maybe_name()? + "x"
```

Invalid because raw errorable values remain disallowed in binary operators.

## 4. Semantics

This proposal intentionally chooses a byte-oriented core model.

- `len(str)` returns the number of bytes in the string
- `a == b` and `a != b` compare strings by exact byte equality
- `a + b` returns a new string containing the bytes of `a` followed by the bytes
  of `b`
- `s[i]` returns the byte value at offset `i` as `i32`
- `s[i:j]` returns the substring covering byte offsets `[i, j)`

Indexing and slicing are over byte offsets, not Unicode scalar positions. That
keeps the core string model simple and matches what a lexer typically needs.

Out-of-range string index or slice operations should trap with a clear runtime
panic message rather than producing silently invalid results.

## 5. Type Rules

- `len(str)` is valid and returns `i32`
- `==` and `!=` are valid for `str` when both operands are `str`
- `+` is valid for `str + str` and returns `str`
- `s[i]` requires `s` to be `str` and `i` to be an integer
- `s[i:j]` requires `s` to be `str` and both bounds to be integers
- raw errorable values remain disallowed in string indexing, slicing, and binary
  operators unless first handled with existing error forms

## 6. Grammar / Parsing Shape

- no new keyword is required
- extend existing postfix indexing syntax to recognize a slicing form with `:`
- extend `+` typing to support `str + str`
- extend equality typing to support `str == str` and `str != str`
- extend builtin `len` to accept `str`

Examples:

- `src[i]`
- `src[start:end]`
- `"a" + "b"`
- `name == "main"`

## 7. Lowering / Implementation Model

- parser: add string slicing syntax in postfix parsing
- AST / IR: either reuse `IndexExpr` plus a new `SliceExpr`, or add a dedicated
  string-slice node
- checker: extend builtin `len`, binary operator typing, and indexing rules
- codegen: reuse the existing `%yar.str` representation for slicing by adjusting
  pointer and length
- runtime: add string concatenation support and panic messages for bounds checks

The current runtime already knows how to pass strings as pointer-plus-length.
That makes `len(str)` and slicing natural additions. Concatenation is the main
new runtime cost because it needs allocation.

## 8. Interactions

- errors: no change to the explicit error model
- structs: strings remain storable in struct fields
- arrays: byte-oriented string slicing is intentionally separate from array
  slicing for now
- control flow: improves expression writing in lexers and diagnostics
- returns: strings return as normal values
- builtins: `len` becomes less array-specific
- future modules/imports: makes text-heavy package code practical
- future richer type features: later rune helpers or char literals can build on
  this byte-level base

## 9. Alternatives Considered

### Keep strings opaque longer

Rejected because it blocks self-hosting and many ordinary text-processing tasks.

### Rune-based indexing

Rejected for the first version because it adds more complexity and hides the
actual storage model a lexer often needs.

### Return a one-byte string from `s[i]`

Rejected because returning a numeric byte value is more explicit and avoids
pretending that every single byte is a meaningful standalone string.

## 10. Complexity Cost

- language surface: medium
- parser complexity: low to medium
- checker complexity: medium
- lowering/codegen complexity: medium
- runtime complexity: medium
- diagnostics complexity: low
- test burden: medium
- documentation burden: medium

## 11. Why Now?

Strings are currently too weak for a self-hosted lexer and too awkward for good
diagnostics. This is a direct capability gap, not a speculative convenience.

## 12. Open Questions

- Should YAR later add character literals for readable byte comparisons?
- Should array slicing, if added later, intentionally match string slicing
  syntax?
- What exact panic text should out-of-range operations use?

## 13. Decision

Accepted.

The compiler now supports byte-oriented string operations: `len(str)` returns
byte count, `==` and `!=` compare by exact byte equality, `+` concatenates with
allocation, `s[i]` returns byte value as `i32`, and `s[i:j]` returns a byte
substring. Out-of-range indexing and slicing trap at runtime.

## 14. Implementation Checklist

- [x] parser (reuses existing postfix indexing and slicing syntax)
- [x] AST / IR updates (no new nodes needed)
- [x] checker
- [x] codegen
- [x] runtime string concat and bounds checks
- [x] diagnostics
- [x] tests
- [x] `current-state.md` update
- [x] `decisions.md` update
