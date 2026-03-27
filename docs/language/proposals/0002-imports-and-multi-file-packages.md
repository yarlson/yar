# Proposal: Imports and Multi-File Packages

Status: exploring

## 1. Summary

Add code organization beyond one-file programs.

## 2. Motivation

The current one-file restriction keeps the language simple, but it also sharply
limits program size and maintainability.

## 3. User-Facing Examples

This proposal is still exploratory. Concrete syntax is intentionally not fixed
yet.

## 4. Semantics

Open.

Main questions include:

- package identity
- file membership rules
- import syntax
- symbol visibility
- builtin interactions
- cycle handling

## 5. Type Rules

Open.

## 6. Grammar / Parsing Shape

Open.

## 7. Lowering / Implementation Model

Would require significant frontend and build pipeline work.

## 8. Interactions

Heavy interactions with:

- symbol resolution
- builtins
- future stdlib organization
- future visibility rules
- compiler invocation model

## 9. Alternatives Considered

### Stay one-file longer

Still plausible if other features are more urgent.

### Add multi-file without imports

Possible, but likely too limited and potentially confusing.

## 10. Complexity Cost

High.

## 11. Why Now?

Not yet decided.

## 12. Open Questions

Many.

## 13. Decision

Deferred for now.

## 14. Implementation Checklist

Not applicable yet.
