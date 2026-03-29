# Proposal: Map Key Enumeration

Status: accepted

## 1. Summary

Add one builtin for enumerating map keys:

- `keys(map[K]V) []K`

This is intentionally smaller than a full iterator protocol or dedicated map
loop syntax.

## 2. Motivation

Current maps are good for lookup, insertion, deletion, and membership, but they
are not enumerable.

That is a practical limitation for compiler and tooling code. A self-hosted
frontend routinely needs to:

- walk symbol tables
- collect import names
- gather error-name sets before assigning stable codes
- inspect visited sets and package maps
- turn internal maps into deterministic output by sorting the keys

Without a way to extract keys, maps remain useful only as hidden side tables.
That forces awkward duplicate slice maintenance just to recover names already
stored in the map.

## 3. User-Facing Examples

### Valid examples

```
fn names(symbols map[str]i32) []str {
    return keys(symbols)
}
```

```
fn dump(symbols map[str]i32) !void {
    names := keys(symbols)
    for i := 0; i < len(names); i = i + 1 {
        name := names[i]
        value := symbols[name]?
        print(name)
        print("\n")
        print_int(value)
        print("\n")
    }
}
```

### Invalid examples

```
ks := keys(values)
```

Invalid when `values` is not a map.

```
ks := keys(maybe_map()?)
```

Invalid because raw errorable expressions remain disallowed in builtin argument
position.

```
ks := keys(m)
ks[0] = true
```

Invalid when `m` is a `map[str]i32`, because `keys(m)` has type `[]str`.

## 4. Semantics

`keys(m)` returns one slice containing every key currently present in `m`.

The first version guarantees:

- each present key appears exactly once
- absent keys do not appear
- the returned slice is a snapshot, not a live iterator view

The first version intentionally does not guarantee:

- any particular key order
- any relation to insertion order
- any relation to hash-bucket order that remains stable across runs

This proposal deliberately chooses snapshot enumeration because it composes well
with existing slices and `for` loops. It also avoids introducing iterator state,
range syntax, or mutation-during-iteration rules in the first version.

`keys(m)` may allocate a new slice. That allocation follows the existing
runtime-managed heap model.

## 5. Type Rules

- `keys(m)` is well-typed only when `m` has type `map[K]V`
- `keys(m)` returns `[]K`
- the returned key type is the same map key type already validated by the map
  rules
- `keys` does not alter the missing-key behavior of `m[k]`, which remains `!V`

## 6. Grammar / Parsing Shape

No new grammar is required.

This is a new builtin with ordinary call syntax:

- `keys(m)`

The builtin is compiler-owned and not user-overridable, matching the current
approach for `len`, `append`, `has`, and `delete`.

## 7. Lowering / Implementation Model

- parser: no changes
- AST / IR: no new syntax nodes beyond ordinary builtin call representation
- checker: add a builtin signature rule `keys(map[K]V) []K`
- codegen: lower to a runtime helper that materializes one slice of keys
- runtime: add map-key extraction support for each supported key family

The runtime work is smaller than full map iteration because the runtime only
needs to gather keys into one returned slice. The rest of the control flow stays
in ordinary YAR loops over slices.

## 8. Interactions

- errors: `keys` itself is non-errorable; map lookup semantics stay unchanged
- structs: no special interaction
- arrays: no special interaction
- control flow: works with existing `for` loops over `len(keys(m))`
- returns: key slices return like other slice values
- builtins: extends the current family of compiler-owned map helpers
- future modules/imports: package and symbol-table enumeration depend on this
- future richer type features: a later general map iterator can still build on
  this smaller foundation

## 9. Alternatives Considered

### Add full map iteration syntax immediately

Rejected for now because it adds a larger control-flow feature with more
questions about ordering, aliasing, and mutation during iteration.

### Keep maps non-enumerable and require side slices

Rejected because it makes compiler data structures awkward and error-prone for a
core self-hosting use case.

### Add `values(m)` or `entries(m)` immediately

Deferred because `keys(m)` is the smallest capability that unlocks practical map
enumeration. Values remain accessible through ordinary indexed lookup once the
key set is known.

## 10. Complexity Cost

- language surface: low to medium
- parser complexity: low
- checker complexity: low
- lowering/codegen complexity: medium
- runtime complexity: medium
- diagnostics complexity: low
- test burden: medium
- documentation burden: medium

## 11. Why Now?

The current maps proposal explicitly stopped short of enumeration. That was a
reasonable first cut, but it leaves one real compiler-shaped gap behind.

`keys(m)` is a small, honest extension that unlocks map traversal without
committing YAR to a larger iterator model yet.

## 12. Open Questions

- Should the builtin be named `keys`, `map_keys`, or something more explicit?
- Should `keys(m)` preserve any stable order for small maps, or should the order
  remain explicitly unspecified?
- Should a later `values(m)` or `entries(m)` be required before self-hosting, or
  is `keys(m)` enough?

## 13. Decision

Accepted.

This is the smallest believable map-enumeration capability for self-hosting
without expanding the control-flow surface unnecessarily.

## 14. Implementation Checklist

- [x] builtin signature and checker support
- [x] runtime key extraction
- [x] lowering/codegen hook
- [x] map integration tests
- [x] diagnostics for misuse
- [x] `current-state.md` update
- [x] `decisions.md` update
