# Proposal: <feature name>

Status: exploring | proposed | accepted | rejected | deferred | withdrawn

Implementation: not started | partial | implemented | removed

## 1. Summary

Briefly describe the feature and its boundary.

## 2. Motivation

What concrete pain, limitation, inconsistency, or missing capability exists in
the language today? Use current examples where possible.

## 3. User-Facing Examples

Provide small valid and invalid examples and explain every invalid case.

## 4. Semantics

Define what the feature does, when it applies, its values and types, control
flow, guarantees, limits, and failure behavior.

## 5. Type Rules

Define what is well-typed, what is rejected, and every declaration, statement,
expression, operand, and result restriction.

## 6. Grammar / Parsing Shape

Describe syntax, precedence, ambiguity, and parser impact. Say `None` when the
feature adds no grammar.

## 7. Lowering / Implementation Model

Cover parser, AST/IR, checker, lowering/codegen, runtime, platform, and migration
impact as applicable. This section is implementation guidance, not current-state
authority.

## 8. Interactions

Cover errors, data types, control flow, concurrency, resources, packages,
standard library, tooling, and likely future features.

## 9. Alternatives Considered

Describe at least two plausible alternatives and why they were not selected.

## 10. Complexity Cost

Evaluate language surface, parser, checker, lowering/codegen, runtime,
diagnostics, testing, documentation, platform, and operational cost.

## 11. Why Now?

Explain why this belongs in the current plan rather than later.

## 12. Open Questions

List unresolved questions. Acceptance requires that none can silently change the
public contract.

## 13. Decision

Record Accepted, Rejected, Deferred, or Withdrawn with a concise rationale.

## 14. Acceptance and Implementation Checklist

- parser / AST / checker / lowering / codegen changes as applicable
- runtime and platform changes as applicable
- positive, negative, boundary, and failure-path tests
- representative `testdata/` update when behavior changes
- `docs/YAR.md` update for public current behavior
- affected `docs/context/` updates for internal current behavior
- `LLM.txt` derived-reference update
- `decisions.md` update when decision rationale changes
- synchronized proposal-registry update after all acceptance evidence is complete
