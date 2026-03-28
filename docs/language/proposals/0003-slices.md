# Proposal: Slices

Status: deferred

## 1. Summary

Add dynamically-sized slice values as a more flexible sequence type than
fixed-size arrays.

The smallest useful version is:

- slice types `[]T`
- slice literals
- `len(slice)`
- indexing and index assignment
- slicing `s[i:j]`
- `append(slice, value) []T`

## 2. Motivation

Fixed arrays are useful, but they are too rigid for many practical programs.

Frontend self-hosting creates especially strong pressure here. The current Go
frontend repeatedly grows variable-length collections such as:

- token lists
- diagnostics
- parameter lists
- block statements
- struct literal fields
- call arguments
- scope stacks

Without slices or another dynamic sequence story, YAR cannot comfortably express
the core accumulation patterns that a compiler frontend needs.

## 3. User-Facing Examples

### Valid examples

```yar
fn collect() []i32 {
    values := []i32{}
    values = append(values, 1)
    values = append(values, 2)
    return values
}

fn second(values []i32) i32 {
    return values[1]
}

fn prefix(values []i32, n i32) []i32 {
    return values[0:n]
}
```

### Invalid examples

```yar
xs := []void{}
```

Invalid because `void` is not a storable element type.

```yar
xs := []i32{}
xs = append(xs, true)
```

Invalid because appended values must match the slice element type.

```yar
x := xs[true]
```

Invalid because slice indexing requires an integer index.

## 4. Semantics

A slice `[]T` is a runtime-sized view over a sequence of `T` values.

The minimal model is intentionally close to a value descriptor:

- a slice carries pointer, length, and capacity-like runtime state
- indexing reads or writes an existing element
- slicing `s[i:j]` produces a new slice view over the same underlying storage
- `append(s, v)` returns an updated slice value and may allocate new storage

The caller must keep the returned result of `append`.

```yar
xs = append(xs, 1)
```

That keeps mutation visible and avoids hidden parameter mutation rules.

Out-of-range indexing or slicing traps with a runtime panic rather than silently
producing invalid memory behavior.

The first version does not include:

- user-visible capacity inspection
- dedicated reserve/grow builtins
- variadic append
- slice iteration syntax beyond existing loops
- implicit array-to-slice conversion

## 5. Type Rules

- `[]T` is well-typed when `T` is a first-class storable value type other than
  `void` and `noreturn`
- `[]T{...}` requires every element to be assignable to `T`
- `s[i]` requires `s` to be `[]T` and `i` to be an integer, and has type `T`
- `s[i] = v` requires `v` to be assignable to `T`
- `s[i:j]` requires `s` to be `[]T` and both bounds to be integers, and has type
  `[]T`
- `append(s, v)` requires `s` to be `[]T` and `v` to be assignable to `T`, and
  returns `[]T`
- `len(s)` is valid for `[]T` and returns `i32`

## 6. Grammar / Parsing Shape

- extend type parsing to support `[]T`
- add slice literal syntax `[]T{...}`
- extend existing postfix `[...]` syntax to recognize a slicing form with `:`

Examples:

- `[]i32`
- `[]User{}`
- `xs[i]`
- `xs[i:j]`
- `append(xs, value)`

## 7. Lowering / Implementation Model

- parser: add slice types, slice literals, and slice syntax
- AST / IR: add slice type representation and likely a dedicated slice-expression
  node
- checker: validate element typing, append typing, and indexing/slicing rules
- codegen: lower slices to a runtime descriptor representation
- runtime: provide allocation, growth, bounds checks, and append support

Unlike fixed arrays, slices require runtime support for dynamic sizing and
possible reallocation. That is the main reason this feature is materially more
expensive than arrays.

## 8. Interactions

- errors: no change to the explicit error model
- structs: slices should be valid struct fields
- arrays: arrays remain fixed-size value types; slices are the dynamic sequence
  companion type
- control flow: existing `for` remains sufficient together with `len`
- returns: slices return as normal values
- builtins: `len` becomes a shared sequence builtin rather than array-only
- future modules/imports: slices are important for practical library and package
  code
- future richer type features: boxes and enums should compose naturally as slice
  element types

## 9. Alternatives Considered

### Keep arrays only for longer

Simpler, but it keeps many practical accumulation patterns awkward or impossible.

### Add a growable vector type instead of slices

Possible, but slices align better with indexing, slicing, and lightweight views.

### Make `append` mutate the slice variable implicitly

Rejected because explicit reassignment makes growth behavior easier to read and
reason about.

## 10. Complexity Cost

- language surface: medium
- parser complexity: medium
- checker complexity: medium
- lowering/codegen complexity: medium to high
- runtime complexity: high
- diagnostics complexity: medium
- test burden: high
- documentation burden: medium

## 11. Why Now?

Dynamic sequences are one of the clearest self-hosting blockers.

Even so, the feature remains deferred because it brings a real runtime and
aliasing model with it, and YAR should not add that lightly.

## 12. Open Questions

- Should arrays gain explicit conversion to slices in the first version?
- Should empty slice literals require an explicit type every time?
- Should future versions expose capacity, or keep it entirely implicit?
- How much aliasing guidance should the language document explicitly?

## 13. Decision

Deferred.

Slices are clearly valuable and likely necessary for self-hosting, but they have
enough runtime and semantic weight that they should not be rushed in before the
core language is more settled.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- runtime sequence support
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
