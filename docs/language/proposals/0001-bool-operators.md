# Proposal: Boolean Operators `&&` and `||`

Status: accepted

## 1. Summary

Add short-circuit boolean operators:

- `&&`
- `||`

## 2. Motivation

The language already has:

- `bool`
- unary `!`
- comparison operators
- `if`, `else`, and loops

Without `&&` and `||`, boolean expressions are unnecessarily limited and common
conditions become awkward.

## 3. User-Facing Examples

### Valid examples

```
if ok && ready {
    return 0
}

if user.id > 0 || debug {
    print("go\n")
}
```

### Invalid examples

```
x := 1 && 2
```

Invalid because `&&` requires boolean operands.

```
x := maybe() && ok
```

Invalid because raw errorable values cannot be used directly in binary operators.

## 4. Semantics

`&&` and `||` are short-circuiting boolean operators.

- `a && b` evaluates `b` only if `a` is true
- `a || b` evaluates `b` only if `a` is false

Both operands must have type `bool`.
The result type is `bool`.

## 5. Type Rules

- left operand of `&&` and `||` must be `bool`
- right operand of `&&` and `||` must be `bool`
- result type is `bool`

## 6. Grammar / Parsing Shape

Add `&&` and `||` with standard precedence:

- `||` lower than `&&`
- both lower than comparison
- both higher than assignment-level statement structure

## 7. Lowering / Implementation Model

Lower with short-circuit control flow, not eager evaluation.

## 8. Interactions

- raw errorables remain disallowed in binary operators
- fits naturally with existing `if` and loop conditions
- no runtime changes required

## 9. Alternatives Considered

### Eager boolean operators

Rejected because short-circuit behavior is the expected and useful model.

### Keyword forms

Rejected because symbolic forms are clearer and conventional here.

## 10. Complexity Cost

Low.

## 11. Why Now?

This fills an obvious language hole and improves everyday expressiveness.

## 12. Open Questions

None currently.

## 13. Decision

Accepted and implemented.

## 14. Implementation Checklist

- parser
- checker
- lowering/codegen
- tests
- `current-state.md` update
- `decisions.md` update
