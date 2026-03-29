# Proposal: Garbage Collection

Status: exploring

## 1. Summary

Consider replacing or extending the current minimal runtime-managed allocation
model with explicit garbage collection.

Possible directions include:

- tracing collection for heap-managed objects
- a conservative or precise collector
- collection that remains invisible to user code

## 2. Motivation

Current YAR already has runtime-managed allocation for:

- pointers
- slices
- maps
- string concatenation and other heap-backed helpers

But it does not currently define reclamation beyond process lifetime.

That is acceptable for the current stage, especially for compiler and tooling
programs, but longer-lived programs or more allocation-heavy patterns may
eventually create pressure for reclamation.

Garbage collection could improve long-running program behavior, but it would
also turn a deliberately minimal memory story into a much larger runtime
commitment.

## 3. User-Facing Examples

### Valid examples

```
fn main() i32 {
    values := []i32{}
    values = append(values, 1)
    values = append(values, 2)
    return 0
}
```

Valid because the user-facing semantics would remain the same even if a garbage
collector existed under the runtime.

### Invalid examples

```
gc()
```

Invalid in the smallest version because collection would remain a runtime
concern, not a user-visible builtin.

```
free(values)
```

Invalid because a GC design would not imply manual deallocation.

## 4. Semantics

The user-facing semantics could remain largely unchanged:

- allocation remains runtime-managed
- user code still does not free memory directly
- pointer, slice, map, and string behavior stay source-compatible

The major semantic questions are runtime-facing:

- when collection may happen
- whether finalizers or weak references exist
- whether collection pauses are observable
- whether current pointer and environment representations remain valid

## 5. Type Rules

Garbage collection does not necessarily require new source-level type rules.

Possible exceptions:

- if non-collected regions or pinning are ever exposed
- if future unsafe features appear

The smallest version should avoid adding new type-system surface.

## 6. Grammar / Parsing Shape

No new syntax is required for the smallest version.

If any syntax exists, the design is no longer a minimal GC proposal.

## 7. Lowering / Implementation Model

- parser impact may be none
- AST / IR impact may be none or minimal
- checker impact may be none in the smallest version
- codegen may need to emit metadata or use a collector-aware allocation path
- runtime impact is high and is the core of the feature

## 8. Interactions

- errors: allocation failure and GC failure must remain outside the ordinary
  `error` model unless the language changes significantly
- structs: heap-embedded references would need scanning support
- arrays: arrays containing references would need scanning support
- control flow: no direct source-level interaction
- returns: escaping values remain valid under the runtime model
- builtins: existing allocation-backed builtins would route through collector-aware paths
- future modules/imports: no direct interaction
- future richer type features: closures and interfaces would likely increase GC pressure

## 9. Alternatives Considered

- keep the current minimal runtime-managed model
  - cheapest and most coherent with the current language stage
- add region or arena-style manual lifetime tools
  - more explicit
  - likely too heavy and user-visible for current YAR
- add tracing garbage collection
  - stronger long-running runtime story
  - much higher runtime complexity

## 10. Complexity Cost

- language surface: low in the smallest version
- parser complexity: none
- checker complexity: none to low
- lowering/codegen complexity: moderate
- runtime complexity: very high
- diagnostics complexity: low to moderate
- test burden: high
- documentation burden: moderate

## 11. Why Now?

Garbage collection should not land by accident as a runtime-only implementation
detail. Writing the proposal now makes the tradeoff explicit: better reclamation
versus a much larger runtime commitment.

## 12. Open Questions

- is collection necessary before long-running programs are a real target?
- can the current runtime representations be scanned precisely?
- should GC wait until closures or interfaces exist?
- does GC fit YAR’s current identity, or is the minimal model enough?

## 13. Decision

Exploring. Garbage collection is a major runtime choice and is not currently
justified by the implemented language scope alone.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
