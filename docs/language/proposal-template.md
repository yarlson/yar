# Proposal: <feature name>

Status: exploring | proposed | accepted | rejected | deferred

## 1. Summary

Briefly describe the feature.

## 2. Motivation

What concrete pain, limitation, inconsistency, or missing capability exists in
the language today?

Use current language examples where possible.

## 3. User-Facing Examples

Provide small examples of intended usage.

### Valid examples

```yar
// example
```

### Invalid examples

```yar
// example
```

Explain why each invalid example is invalid.

## 4. Semantics

Describe precisely what the feature means.

Answer:

- what it does
- when it applies
- what control flow it causes
- what values and types are involved
- what the language guarantees

## 5. Type Rules

Describe:

- what is well-typed
- what is not well-typed
- constraints on operands / operands and results
- restrictions in declarations, statements, and expressions

## 6. Grammar / Parsing Shape

Describe the syntax form and any precedence or ambiguity concerns.

## 7. Lowering / Implementation Model

Explain how the feature maps into existing compiler structures.

Cover:

- parser impact
- AST / IR impact
- checker impact
- codegen impact
- runtime impact, if any

## 8. Interactions

Describe how the feature interacts with existing or likely-future areas:

- errors
- structs
- arrays
- control flow
- returns
- builtins
- future modules/imports
- future richer type features

## 9. Alternatives Considered

List at least two alternatives and why they were not chosen.

## 10. Complexity Cost

Evaluate cost in:

- language surface
- parser complexity
- checker complexity
- lowering/codegen complexity
- runtime complexity
- diagnostics complexity
- test burden
- documentation burden

## 11. Why Now?

Why does this belong in the current milestone instead of later?

## 12. Open Questions

List unresolved questions, if any.

## 13. Decision

Accepted / Rejected / Deferred, with a short explanation.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
