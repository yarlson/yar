# Proposal: Enums / Tagged Unions

Status: exploring

## 1. Summary

Add richer sum-like data modeling.

## 2. Motivation

Structs model product types well, but the language currently lacks a direct way
to model disjoint alternatives in domain data.

## 3. User-Facing Examples

Not fixed yet.

## 4. Semantics

Open.

Key questions:

- plain enums vs tagged unions
- payload support
- construction syntax
- matching / branching syntax
- exhaustiveness expectations

## 5. Type Rules

Open.

## 6. Grammar / Parsing Shape

Open.

## 7. Lowering / Implementation Model

Potentially substantial.
Would likely affect both type representation and control-flow ergonomics.

## 8. Interactions

Heavy interactions with:

- structs
- arrays
- errors
- future pattern matching
- future methods
- equality semantics

## 9. Alternatives Considered

### Model alternatives with structs and sentinel fields

Possible, but clumsy.

### Defer entirely

Current preferred stance until stronger need appears.

## 10. Complexity Cost

High.

## 11. Why Now?

Not currently justified.

## 12. Open Questions

Many.

## 13. Decision

Deferred for now.

## 14. Implementation Checklist

Not applicable yet.
