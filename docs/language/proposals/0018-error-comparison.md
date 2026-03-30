# Proposal: Error Comparison and Error Expressions

Status: accepted

## 1. Summary

Allow error values to be compared with `==` and `!=`, and allow `error.Name`
expressions to appear outside return statements.

The implemented version supports:

- `==` and `!=` on two error-typed operands
- `error.Name` as a standalone expression usable in variables, arguments, and
  comparisons
- internal representation as `i32` codes with `icmp` comparison

## 2. Motivation

Before this change, `error.Name` expressions were only valid in return
statements, and error values could not be compared directly. This made it
impossible to write patterns like:

```
if err == error.NotFound {
    // handle missing
}
```

or pass error values as arguments:

```
testing.equal[error](t, err, error.NotFound)
```

The testing package needed type-specific assertion functions because generic
`testing.equal[V]` could not accept error values. Removing this restriction
unified the assertion surface and made error handling more expressive without
adding new syntax.

## 3. User-Facing Examples

### Valid examples

```
fn lookup(key str) !str {
    if key == "" {
        return error.InvalidInput
    }
    return error.NotFound
}

fn main() i32 {
    result := lookup("x") or |err| {
        if err == error.NotFound {
            print("not found")
        }
        return 1
    }
    return 0
}
```

```
expected := error.NotFound
testing.equal[error](t, got_err, expected)
```

### Invalid examples

```
x := error.NotFound == 42
```

Invalid because comparison requires both operands to be error-typed. Error
values cannot be compared with integers even though they are `i32` internally.

```
err := error.NotFound
result := err + error.InvalidInput
```

Invalid because errors do not support arithmetic. Only `==` and `!=` are
defined.

## 4. Semantics

- `error.Name` produces a value of type `error`
- `error.Name` is valid in any expression position, not only return statements
- `==` and `!=` compare two `error`-typed operands and produce `bool`
- comparison is by identity: two errors are equal if and only if they have the
  same error name
- error values are non-errorable: `error.NotFound` has type `error`, not `!error`
- the internal representation is an `i32` code, but this is not observable from
  source code

## 5. Type Rules

- both operands of `==` / `!=` must have base type `error`
- the result type is `bool`
- `error.Name` has type `error` regardless of expression position
- errorable types (`!T`) cannot be compared directly; unwrap first
- no implicit conversion between `error` and integer types

## 6. Grammar / Parsing Shape

No new syntax. `error.Name` already exists in the grammar for return
statements. This change lifts the restriction on where that expression may
appear.

The parser continues to parse `error.Name` as an `ErrorLiteral` AST node. The
checker no longer rejects it in non-return positions.

## 7. Lowering / Implementation Model

- parser impact: none; `error.Name` was already parsed
- AST / IR impact: none; `ErrorLiteral` node is reused
- checker impact: removes the restriction that `error.Name` must appear in a
  return statement; adds `==` / `!=` support for the `error` base type
- codegen impact: emits `icmp eq` or `icmp ne` on the `i32` error codes, same
  as integer comparison
- runtime impact: none

## 8. Interactions

- errors: directly extends the error model with comparison capability
- structs: no interaction
- arrays: no interaction
- control flow: enables `if err == error.Name` patterns
- returns: `error.Name` in return statements continues to work unchanged
- builtins: `to_str` accepts error values and uses the comparison machinery
  internally for the switch-based string conversion
- future modules/imports: no interaction
- future richer type features: no interaction

## 9. Alternatives Considered

- keep error comparison implicit through string conversion and string comparison
  - works but is slower and less clear
  - loses the type safety of error-to-error comparison
- add a dedicated `errors.is()` function
  - adds unnecessary indirection for what is a simple value comparison
  - diverges from how other value types (`i32`, `bool`) support `==`

## 10. Complexity Cost

- language surface: low
- parser complexity: none
- checker complexity: low
- lowering/codegen complexity: low
- runtime complexity: none
- diagnostics complexity: low
- test burden: low
- documentation burden: low

## 11. Why Now?

Error comparison was needed to unify the testing assertion surface. Without it,
`testing.equal[V]` could not accept error values, forcing type-specific
assertion functions. It also enables idiomatic error-handling patterns that were
awkward to express.

## 12. Open Questions

None. The feature is implemented and stable.

## 13. Decision

Accepted and implemented. Error values support `==` and `!=` comparison, and
`error.Name` expressions are valid in any expression position. The internal
representation remains `i32` codes with `icmp` comparison.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `docs/context` update
- [x] `decisions.md` update
