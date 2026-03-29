# Proposal: Sorting Helpers

Status: accepted

## 1. Summary

Add a small `sort` standard-library package for deterministic compiler and
tooling work.

The first version is intentionally narrow:

- `sort.strings(values []str) void`
- `sort.i32s(values []i32) void`
- `sort.i64s(values []i64) void`

Each function sorts the given slice in place in ascending order.

## 2. Motivation

A self-hosted compiler needs deterministic ordering even when its internal data
structures are unordered.

Examples from compiler work include:

- sorting `.yar` file names before parsing
- sorting package paths for stable lowering order
- sorting error names before assigning numeric codes
- sorting map keys before emitting diagnostics or generated output

Today YAR has slices and maps, but no generic functions, no function values, and
no existing sort library. That makes user-defined reusable sorting awkward.

One-off hand-written sorting loops are possible, but that would duplicate a
foundational piece of tooling logic across programs and packages.

## 3. User-Facing Examples

### Valid examples

```
import "sort"

fn stable_files(files []str) []str {
    sort.strings(files)
    return files
}
```

```
import "sort"

fn stable_error_codes(codes []i32) []i32 {
    sort.i32s(codes)
    return codes
}
```

### Invalid examples

```
sort.strings(names[0])
```

Invalid because `sort.strings` requires `[]str`, not `str`.

```
sort.i32s(values)
```

Invalid when `values` is not `[]i32`.

```
sort.values(names)
```

Invalid because the first version intentionally ships only a very small fixed
API.

## 4. Semantics

Each sort helper mutates the passed slice in place.

- `sort.strings` sorts by exact byte-string ordering
- `sort.i32s` sorts by numeric ascending order
- `sort.i64s` sorts by numeric ascending order

The first version does not specify:

- algorithm stability
- asymptotic complexity guarantees beyond “reasonable implementation quality”
- custom comparators
- descending order helpers

The first version does specify:

- the result is a permutation of the original slice values
- the resulting slice is ordered ascending by the relevant comparison rule
- sorting an empty or single-element slice is valid and leaves it unchanged

In-place mutation is intentional. It avoids hidden copies and fits the existing
slice model where mutation is already explicit.

## 5. Type Rules

- `sort.strings(values)` is well-typed only for `[]str`
- `sort.i32s(values)` is well-typed only for `[]i32`
- `sort.i64s(values)` is well-typed only for `[]i64`
- all functions return `void`

## 6. Grammar / Parsing Shape

No new grammar is required.

This proposal is entirely standard-library shaped:

- `sort.strings(xs)`
- `sort.i32s(xs)`
- `sort.i64s(xs)`

## 7. Lowering / Implementation Model

- parser: no changes
- AST / IR: no new nodes
- checker: ordinary package-qualified calls
- codegen: ordinary function lowering
- runtime: no required host boundary if the sort package is implemented in pure
  Yar

This proposal is intentionally library-sized, not syntax-sized.

The likely implementation is a simple in-place comparison sort in Yar itself.
That keeps the language surface small while still standardizing a crucial piece
of deterministic tooling behavior.

## 8. Interactions

- errors: sorting is non-errorable
- structs: later proposals may motivate slice-of-struct sorting helpers, but the
  first version stays scalar-only
- arrays: users can still copy array values into slices before sorting if needed
- control flow: no new control-flow forms
- returns: sorted slices return like ordinary values
- builtins: no new builtin is required
- future modules/imports: deterministic package and diagnostics order depends on
  a small sorting library
- future richer type features: later generics or comparator forms may subsume
  this package, but this small surface remains useful before those features

## 9. Alternatives Considered

### Wait for generics and higher-order functions

Rejected because that would delay a practical self-hosting need behind much
larger language work.

### Force every program to hand-roll its own sorting

Rejected because deterministic ordering is core compiler infrastructure, not an
interesting place for repeated boilerplate.

### Add builtin sort operations

Rejected because sorting belongs naturally in stdlib and does not require new
syntax or compiler-owned polymorphism.

## 10. Complexity Cost

- language surface: low
- parser complexity: low
- checker complexity: low
- lowering/codegen complexity: low
- runtime complexity: low
- diagnostics complexity: low
- test burden: medium
- documentation burden: medium

## 11. Why Now?

Self-hosting needs deterministic ordering long before it needs large type-system
features such as generics.

This proposal standardizes the smallest useful sorting surface for compiler-like
programs while staying within the current language model.

## 12. Open Questions

- Should the first version also include `sort.bools`, or is that unnecessary?
- Should `sort.strings` use byte ordering only, or should locale-sensitive
  behavior ever exist? The current answer should likely stay “byte ordering
  only.”
- Should a later proposal add `sorted_copy` helpers, or is in-place mutation the
  right permanent model?

## 13. Decision

Accepted and implemented.

This is a small library proposal that complements map key extraction and host
filesystem access in the larger self-hosting track while staying entirely in
the stdlib.

## 14. Implementation Checklist

- [x] stdlib package implementation
- [x] integration tests for sorted output
- [x] documentation updates
- [x] `current-state.md` update
- [x] `decisions.md` update
