# Proposal: Garbage Collection

Status: accepted

## 1. Summary

Add runtime-only garbage collection for heap-managed values without changing
the source language.

The accepted design is:

- conservative
- non-moving
- mark-and-sweep
- invisible to user code

## 2. Motivation

YAR already has runtime-managed allocation for:

- pointers
- slices
- maps
- string concatenation and other heap-backed helpers

Closures, interfaces, maps, slices, pointer-backed recursive structures, string
concatenation, and host-backed runtime helpers all increase pressure on that
heap.

The Rust runtime uses one shared allocation boundary. Reclaiming unreachable
blocks behind that boundary keeps longer-running compiler-style and
tooling-style programs viable without widening the language surface.

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

Valid because adding a collector must not change the user-facing semantics.

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

The accepted collector semantics keep the user-facing model unchanged:

- allocation remains runtime-managed
- user code still does not free memory directly
- pointer, slice, map, and string behavior stay source-compatible
- collection may happen during allocation when the runtime-managed heap target
  is exceeded
- user code must not depend on exactly when collection happens
- there are no finalizers, weak references, or user-visible collector hooks
- reachable heap-backed values remain valid across collection
- allocation failure remains an unrecoverable runtime failure outside the
  ordinary `error` model

## 5. Type Rules

Garbage collection adds no new source-level type rules.

- all existing heap-backed values keep their current static types
- there is still no well-typed `gc()` builtin
- there is still no well-typed `free(...)` operation
- the implementation does not expose pinning, regions, or unsafe lifetime
  controls

## 6. Grammar / Parsing Shape

No new syntax is required.

Any implementation must remain runtime-only. User-visible GC or lifetime syntax
would require a separate proposal.

## 7. Lowering / Implementation Model

- parser impact: none
- AST / IR impact: none
- checker impact: none
- codegen impact: the generated native `main` wrapper already passes a
  stack-top pointer through a reserved runtime hook, and existing heap
  operations already lower through shared allocation helpers
- runtime impact: high; the collector captures ABI-preserved registers, scans
  the main stack and managed blocks conservatively, and sweeps unreachable
  blocks

## 8. Interactions

- errors: allocation failure remains outside the ordinary `error` model
- structs: an implementation must scan struct fields stored in heap blocks
- arrays: an implementation must scan arrays stored in heap-managed memory
- control flow: no direct source-level interaction
- returns: escaping values remain valid under the runtime model
- builtins: existing allocation-backed builtins must route through the collector
  without changing their syntax
- future modules/imports: no direct interaction
- future richer type features: closures and interfaces increase the need for
  correct long-running heap behavior

## 9. Alternatives Considered

- keep the current minimal runtime-managed model
  - simpler runtime
  - worse long-running behavior for allocation-heavy programs
- add region or arena-style manual lifetime tools
  - more explicit
  - too user-visible and interaction-heavy for current YAR
- add a precise or moving collector
  - potentially stronger long-term runtime story
  - needs richer metadata and more implementation complexity than the current
    compiler/runtime design warrants

## 10. Complexity Cost

- language surface: low
- parser complexity: none
- checker complexity: none
- lowering/codegen complexity: low to moderate
- runtime complexity: high
- diagnostics complexity: low
- test burden: high
- documentation burden: moderate

## 11. Why Now?

Heap-backed features are already central to the implemented language, and
closures plus interfaces have increased the practical value of reclamation.
Accepting the GC direction now keeps the intended memory story explicit while
the runtime is still small enough to evolve deliberately.

## 12. Open Questions

- should the collector stay conservative, or should future runtime work move
  toward precise metadata?
- does the runtime eventually need generational heuristics or other tuning?
- should any diagnostic or profiling hooks around GC ever become visible?

## 13. Decision

Accepted and implemented in the Rust runtime.

The language surface stays unchanged:

- no `gc()` builtin
- no manual deallocation
- no finalizers

The runtime reclaims unreachable heap-backed storage behind the existing
allocation boundary. Collection is deferred while spawned results remain
unjoined, and live channel slots are explicit roots.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `docs/context` update
- [x] `docs/YAR.md` update
- [x] `docs/language` update
