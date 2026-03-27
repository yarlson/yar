# Proposal: Slices

Status: exploring

## 1. Summary

Add dynamically-sized slice values as a more flexible collection type than
fixed-size arrays.

## 2. Motivation

Fixed arrays are useful but rigid. Many practical programs need a variable-sized
sequence abstraction.

## 3. User-Facing Examples

Examples intentionally omitted for now because syntax and semantics are not yet
settled.

## 4. Semantics

Open.

Key questions:

- representation
- ownership/lifetime expectations
- literal syntax
- slicing syntax
- mutability expectations
- builtin interactions such as `len`

## 5. Type Rules

Open.

## 6. Grammar / Parsing Shape

Open.

## 7. Lowering / Implementation Model

Would likely require runtime representation support and broader checker/codegen
work than fixed arrays.

## 8. Interactions

Heavy interactions with:

- arrays
- indexing
- future loops
- future builtins
- future library surface
- future conversion or borrowing rules, if any

## 9. Alternatives Considered

### Keep arrays only for longer

Simpler, but less flexible.

### Add vectors/list type instead

Possible, but would still require significant design work.

## 10. Complexity Cost

Medium to high.

## 11. Why Now?

Not currently committed.

## 12. Open Questions

Many.

## 13. Decision

Deferred for now.

## 14. Implementation Checklist

Not applicable yet.
