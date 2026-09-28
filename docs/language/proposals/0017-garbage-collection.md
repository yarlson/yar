# Proposal: Garbage Collection

Status: accepted
Implementation: implemented

## 1. Summary

Add runtime-only garbage collection for heap-managed values without changing
the source language.

The accepted design is:

- precise for heap objects, conservative for thread stacks
- non-moving
- stop-the-world mark and bitmap sweep, with parallel marking on large heaps
- active while tasks run
- invisible to user code

The first implementation scanned every byte of every block conservatively,
kept one global block map behind a mutex, and stopped collecting whenever a
task was unjoined. That design retained all garbage in allocation loops
(allocas emitted inside loop bodies kept every iteration's pointers on the
stack) and never collected in long-running servers. The revised runtime below
replaces it.

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
- codegen impact: moderate
  - every allocation passes a pointer-layout descriptor
    `{ i64 stride, i64 count, [count x i64] offsets }` computed from the
    compiler's own type layout; pointer-free layouts pass `null`
  - every `alloca` is placed in the entry block, so loops reuse stack slots
  - each loop condition polls `yar_gc_safepoint_requested` and calls
    `yar_gc_safepoint()` when a collection is pending
  - channel and taskgroup constructors pass their element descriptor
- runtime impact: high
  - 64 KiB pages with 50 size classes up to 32 KiB, page-aligned large-object
    spans, and a two-level page map for constant-time interior-pointer lookup
  - side live/mark bitmaps and per-slot descriptors; objects are zeroed on
    allocation
  - lock-free thread-local allocation from an owned page per size class
  - every Yar thread is a registered mutator; collection stops all of them at
    allocation, loop, or blocking-operation safepoints after spilling
    callee-saved registers, then scans every stack conservatively
  - heap tracing follows descriptors precisely; marking is parallel when the
    previous live heap was at least 8 MiB
  - sweeping swaps mark bits into live bits during the pause; empty pages are
    released
  - the allocation budget between collections is the larger of the live heap
    and a configurable minimum (4 MiB by default)

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
- fully precise stack maps (LLVM statepoints or a shadow stack)
  - would allow a moving collector
  - requires rewriting code generation around GC-aware calls; conservative
    stacks already give precise heap tracing without that cost
- a moving or generational collector
  - better locality and cheaper young-object reclamation
  - needs precise stack roots or pinning, plus write barriers in generated code
- concurrent or incremental marking
  - shorter pauses on large heaps
  - needs write barriers in generated code; deferred until pause times matter

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

- do real workloads need concurrent marking or a generational nursery, which
  would add write barriers to generated code?
- should empty pages be returned to the OS more aggressively than the current
  bounded free-page cache?
- should any diagnostic or profiling hooks around GC ever become visible?

## 13. Decision

Accepted. The delivery state records its implementation in the Rust runtime.

The language surface stays unchanged:

- no `gc()` builtin
- no manual deallocation
- no finalizers

The runtime reclaims unreachable heap-backed storage behind the existing
allocation boundary. Collection runs while tasks are active, traces heap
objects precisely from compiler-emitted layouts, and scans every registered
thread's stack conservatively. Channel buffers are managed objects reached
through their token; task contexts and pending results are explicit roots
until the taskgroup is joined.

Implementation evidence: the runtime memory tests cover reachability, interior
pointers, precise descriptors, finalizers, slot reuse, allocation-pressure
collection, and scanning a blocked thread's stack; codegen tests cover
descriptor emission, entry-block stack slots, and loop safepoint polls; the
fixture runner executes the collection and concurrency fixtures under a 1 KiB
budget.

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
