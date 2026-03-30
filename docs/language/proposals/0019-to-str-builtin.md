# Proposal: `to_str` Builtin

Status: accepted

## 1. Summary

Add a compiler-provided `to_str` builtin that converts primitive values to
their string representation.

The implemented version supports:

- `i32`, `i64`, `bool`, `str`, and `error` argument types
- polymorphic dispatch based on argument type, similar to `len`
- runtime helpers for integer-to-string conversion
- inline selection for bool-to-string
- switch-based mapping for error-to-string

## 2. Motivation

Before `to_str`, the testing package needed separate assertion functions for
each type because there was no uniform way to produce "got X, want Y" failure
messages. The `conv` stdlib package provided `conv.itoa` and `conv.itoa64`, but
those are ordinary functions that cannot handle multiple types polymorphically.

A compiler-provided builtin that accepts any supported primitive and returns
`str` eliminates the need for type-specific assertion helpers and enables
`testing.equal[V]` to provide rich failure messages for all comparable types.

This also replaced `print_int`, a special-case builtin that printed integers
directly. With `to_str`, the only output primitive is `print(str)` and integer
output becomes `print(to_str(n))`.

## 3. User-Facing Examples

### Valid examples

```
print(to_str(42))          // "42"
print(to_str(true))        // "true"
print(to_str("hello"))     // "hello"

n := 100_i64
print(to_str(n))           // "100"

err := error.NotFound
print(to_str(err))         // "error.NotFound"
```

### Invalid examples

```
to_str(some_struct)
```

Invalid because `to_str` only accepts primitive types. Structs, slices, maps,
pointers, enums, and function values are not supported.

```
fn might_fail() !i32 { return 1 }
to_str(might_fail()?)
```

Valid after unwrapping. But:

```
to_str(might_fail())
```

Invalid because errorable values cannot be passed directly. The caller must
unwrap or handle the error first.

## 4. Semantics

- `to_str` accepts exactly one argument
- the argument must be a non-errorable value of type `i32`, `i64`, `bool`,
  `str`, or `error`
- the return type is `str`
- untyped integer literals are coerced to `i32` before conversion
- the conversion is pure: no side effects, no allocation failure exposure
- `to_str` on `str` is an identity operation
- `to_str` on `bool` produces `"true"` or `"false"`
- `to_str` on `error` produces `"error.<Name>"` for known errors and
  `"error.unknown"` for unrecognized codes

## 5. Type Rules

- argument type must be one of: `i32`, `i64`, `bool`, `str`, `error`
- untyped integer arguments are coerced to `i32`
- errorable types are rejected with a diagnostic
- result type is always `str`

## 6. Grammar / Parsing Shape

No new syntax. `to_str` is a builtin function using existing call expression
syntax. The name is resolved during checking as a known builtin, same as
`print`, `len`, and `append`.

## 7. Lowering / Implementation Model

- parser impact: none; parsed as an ordinary call expression
- AST / IR impact: none
- checker impact: special-cases `to_str` alongside other builtins; validates
  argument count, rejects errorable arguments, coerces untyped integers,
  validates base type
- codegen impact:
  - `i32`: emits call to `@yar_to_str_i32(i32) -> %yar.str`
  - `i64`: emits call to `@yar_to_str_i64(i64) -> %yar.str`
  - `bool`: emits `select i1` choosing between `"true"` and `"false"` string
    constants
  - `str`: returns the argument unchanged
  - `error`: emits a `switch` on the error code mapping each known error name
    to its `"error.<Name>"` string constant, with `"error.unknown"` as default
- runtime impact: requires `yar_to_str_i32` and `yar_to_str_i64` C runtime
  functions that allocate and format integer strings

## 8. Interactions

- errors: `to_str(err)` produces the error name as a string; this interacts
  with error comparison (proposal 0018) since the same error code mapping is
  used
- structs: not supported in current version
- arrays: not supported
- control flow: no interaction
- returns: no special interaction
- builtins: complements `print` — `to_str` converts, `print` outputs
- future modules/imports: no interaction
- future richer type features: could be extended to support additional types
  (enums, slices) in future versions

## 9. Alternatives Considered

- extend `print` to accept multiple types directly
  - simpler for output but does not help with string building (e.g., test
    failure messages that concatenate multiple values)
- use only stdlib functions (`conv.itoa`, `conv.itoa64`)
  - works for integers but cannot be polymorphic across types
  - generic `testing.equal[V]` needs a single conversion function that works
    for any `V`
- add a `Stringer` interface
  - too heavyweight for the current language stage
  - requires interface dispatch for a simple conversion

## 10. Complexity Cost

- language surface: low
- parser complexity: none
- checker complexity: low
- lowering/codegen complexity: moderate (type-dependent emission)
- runtime complexity: low (two C helper functions)
- diagnostics complexity: low
- test burden: low
- documentation burden: low

## 11. Why Now?

The testing package needed polymorphic string conversion to provide rich
failure messages in `testing.equal[V]`. Without `to_str`, the testing surface
required separate assertion functions for each type, which was inconsistent
with the generic approach used elsewhere.

## 12. Open Questions

- should `to_str` be extended to support enum types in a future version?
- should struct types with a `to_str` method be supported through a convention
  or interface?

## 13. Decision

Accepted and implemented. `to_str` is a compiler-provided polymorphic builtin
supporting `i32`, `i64`, `bool`, `str`, and `error`. It eliminated
`print_int` and unified the testing assertion surface.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `docs/context` update
- [x] `decisions.md` update
