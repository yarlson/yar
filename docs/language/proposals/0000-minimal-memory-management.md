# Proposal: Minimal Runtime-Managed Memory

Status: accepted

## 1. Summary

Define a minimal shared memory-management model for heap-backed language features.

The core rules are:

- allocation is runtime-managed
- user code does not manually free memory
- allocation failure is not part of the ordinary `error` model
- feature proposals such as pointers, slices, maps, and string concatenation must
  fit one coherent heap story rather than inventing separate ones

This proposal is intentionally foundational. It adds almost no direct syntax by
itself, but it defines the memory model that later heap-using features should
share.

## 2. Motivation

The current language is still mostly value-only.

- fixed arrays are value types
- structs are value types
- direct recursive containment is rejected
- strings already exist, but their current surface avoids most explicit lifetime
  questions

That simplicity breaks as soon as several already-proposed features arrive:

- pointers need allocation for `new(T)`
- slices need backing storage and possible reallocation
- maps need runtime-owned storage
- string concatenation needs new storage

If each of those proposals defines its own lifetime, reclamation, and
allocation-failure story, the language will become inconsistent very quickly.

YAR needs a small explicit memory model before heap-backed features start
landing.

## 3. User-Facing Examples

### Valid examples

```yar
tail := new(Node)
*tail = Node{value: 2, next: nil}

values := []i32{}
values = append(values, 1)

counts := map[str]i32{}
counts["main"] = 1

msg := "line " + itoa(12)
```

All of these may allocate runtime-managed storage.

The user does not explicitly free that storage.

### Invalid examples

```yar
free(tail)
```

Invalid because the minimal memory model does not expose manual deallocation.

```yar
p := new(Node)
q := p + 1
```

Invalid because pointer arithmetic is outside the minimal safe memory model.

```yar
addr := unsafe_addr(node)
```

Invalid because raw address exposure is not part of this design.

## 4. Semantics

This proposal defines a shared rule set for heap-backed values.

The initial intended consumers are:

- `new(T)` allocations for typed pointers
- slice backing storage
- map storage
- string concatenation results
- any later feature that needs heap-owned runtime data

The core semantics are:

- user code may create heap-backed values only through explicit language
  operations defined by other proposals
- the runtime owns allocation and any reclamation strategy
- user code does not explicitly destroy, free, or finalize heap-backed values
- the program must not depend on when unreachable storage is reclaimed
- the first implementation may reclaim only at process exit and still conform to
  this proposal
- a later implementation may use tracing GC or another runtime-managed strategy
  without changing source semantics

As long as a heap-backed object is reachable through ordinary live program
values, operations through those values must remain valid.

This proposal deliberately does not promise:

- prompt reclamation
- destruction order
- finalizers
- user-visible allocator control
- raw addresses
- pointer arithmetic
- pointer-to-integer or integer-to-pointer conversions

Current implementation note:

- the compiler/runtime now has an internal shared allocation boundary through
  runtime helper functions
- that groundwork does not yet add user-visible heap syntax by itself
- the first heap-backed language features should reuse that boundary rather than
  introduce their own

Allocation failure is not modeled as `error` or `!T`.

An allocation failure is a runtime failure, similar in class to other
unrecoverable runtime traps. It should terminate execution with a clear runtime
failure path rather than entering the ordinary explicit error flow.

Heap-backed values remain ordinary first-class values unless a feature proposal
states otherwise. Copying such a value copies the value itself, not necessarily a
deep copy of its underlying storage. Whether underlying storage is shared or
replaced is defined by the feature-specific operation involved.

## 5. Type Rules

This proposal adds no new standalone user-visible type form.

Instead, it constrains future heap-backed features:

- heap-backed values must still have ordinary static types
- heap-backed values are assignable, passable, and returnable like other
  first-class values unless a later proposal explicitly says otherwise
- allocation failure does not produce `error` and does not widen an operation's
  type into `!T`
- there is no well-typed general-purpose `free`, `destroy`, or finalizer hook in
  this model
- there is no well-typed raw address exposure or pointer arithmetic in this
  model

Feature proposals remain responsible for their own detailed typing rules. This
proposal defines the common memory behavior they must fit.

## 6. Grammar / Parsing Shape

This proposal adds no dedicated syntax by itself.

Later proposals may add syntax such as:

- `*T`
- `new(T)`
- `[]T`
- `map[K]V`

Those proposals should reference this memory model rather than redefining
allocation and reclamation semantics independently.

Any future user-visible deallocation, ownership, borrowing, or allocator-control
syntax would require a separate proposal.

## 7. Lowering / Implementation Model

- parser: no direct parser work is required by this proposal alone
- AST / IR: heap-allocating operations introduced by later proposals should be
  explicit in the frontend representation
- checker: later heap-backed features should preserve ordinary static typing and
  must not treat allocation failure as part of the explicit error system
- codegen: heap-allocating operations lower to runtime allocation helpers or
  runtime-managed descriptors
- runtime: the runtime owns object allocation and any reclamation bookkeeping

The smallest viable runtime implementation is a simple runtime-managed allocator,
including a process-lifetime arena or bump allocator.

That choice should be treated as an implementation strategy, not a permanent
language guarantee.

This keeps the first version simple while leaving room for a future collector or
more advanced runtime internals without changing source semantics.

## 8. Interactions

- errors: allocation failure does not use YAR's normal `error` model, so `?` and
  `or |err| { ... }` stay focused on domain/runtime operations that are already
  explicit in function signatures
- structs: structs may contain heap-backed values, and recursive data becomes
  possible only through future indirection features such as pointers
- arrays: fixed arrays remain value types, but their elements may later include
  heap-backed values
- control flow: this proposal adds no ordinary user-visible control-flow form;
  only unrecoverable runtime allocation failure may terminate execution
- returns: heap-backed values return like other first-class values
- builtins: allocation may be triggered by builtins or compiler-owned operations
  such as `new`, `append`, map creation, and string concatenation
- future modules/imports: package boundaries do not need a separate memory model
- future richer type features: pointers, slices, maps, enums, and recursive data
  should all reuse this same runtime-managed memory story

## 9. Alternatives Considered

### Add explicit `free` immediately

Rejected for now because it would force YAR to design ownership, aliasing,
use-after-free hazards, and lifetime rules much earlier than needed.

### Commit to a full tracing GC design now

Rejected for now because it overcommits the runtime before the language has even
settled its first heap-backed features.

### Make leak-until-process-exit the language guarantee

Rejected because it is better as an initial implementation strategy than as a
semantic promise. The language should permit improved reclamation later without
changing user code.

## 10. Complexity Cost

- language surface: low
- parser complexity: low
- checker complexity: low
- lowering/codegen complexity: medium
- runtime complexity: medium
- diagnostics complexity: low to medium
- test burden: medium
- documentation burden: medium

## 11. Why Now?

This belongs now because multiple active proposals already depend on allocation
and runtime-owned storage.

Without a shared memory story:

- pointers define one allocation model
- slices define another
- maps define another
- strings define another

That would create unnecessary redesign pressure and unclear interactions.

The smallest useful move is to decide the common memory baseline now, before the
first heap-backed feature is accepted and implemented.

## 12. Open Questions

- Should the first runtime explicitly document process-exit-only reclamation as a
  temporary implementation note?
- Should string slicing be required to share storage when possible, or remain
  implementation-defined?
- Should future address-of stay out of scope even after `new(T)` exists?
- Should explicit reclamation ever be worth reconsidering in a separate future
  design, or should YAR stay permanently runtime-managed?

## 13. Decision

Accepted.

YAR now treats runtime-managed heap memory as the shared foundation for future
heap-backed features. The internal runtime allocation boundary exists, but this
proposal still does not imply that any user-visible heap feature is already
implemented or shipped.

## 14. Implementation Checklist

- proposal cross-references from heap-backed features
- runtime allocation boundary
- codegen hooks for heap-allocating operations
- diagnostics for unrecoverable allocation failure
- tests
- `current-state.md` update
- `decisions.md` update
