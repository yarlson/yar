# Proposal: Generics

Status: exploring

## 1. Summary

Consider adding parametric polymorphism for reusable data structures and
functions.

The smallest plausible version would likely cover:

- generic functions
- generic structs
- explicit type arguments
- no implicit inference beyond simple local cases unless clearly justified

## 2. Motivation

Current YAR already has recurring type-specific duplication:

- `sort.strings`, `sort.i32s`, and `sort.i64s`
- containers and helpers that would otherwise need one copy per element type
- utility functions that conceptually work for many concrete types

Without generics, reuse either becomes repetitive or pushes pressure toward
bigger builtin libraries. Generics could reduce duplication, but they also
create large interactions with parsing, type checking, code generation,
interfaces, and diagnostics.

## 3. User-Facing Examples

### Valid examples

```yar
struct Box[T] {
    value T
}

fn first[T](values []T) T {
    return values[0]
}
```

### Invalid examples

```yar
fn first[T](values []T) void {
    return values[0]
}
```

Invalid because the declared return type does not match the returned value.

```yar
struct Bad[T] {
    value void
}
```

Invalid because `void` is still not a storable field type.

## 4. Semantics

Generics would introduce type parameters that are substituted with concrete
types at use sites.

The design must settle:

- whether monomorphization or dictionary-style lowering is used
- whether all type arguments must be explicit
- how generic code interacts with current builtins such as `len` and `append`
- what constraints, if any, exist in the first version

The smallest coherent design may require explicit type arguments and a very
limited constraint system, or no constraints at all in the first cut.

## 5. Type Rules

- each type parameter has a scope
- generic declarations must remain well-formed under their parameter list
- current invalid element or field types remain invalid after substitution
- instantiations must provide the expected number of type arguments
- if constraints exist, each type argument must satisfy them

## 6. Grammar / Parsing Shape

Candidate syntax:

```yar
fn first[T](values []T) T { ... }
struct Box[T] { value T }
```

The parser must handle:

- type parameter lists after names
- type argument lists at use sites
- disambiguation from existing indexing and selector syntax

## 7. Lowering / Implementation Model

- parser adds type parameter and type argument syntax
- AST must represent generic declarations and instantiations
- checker must introduce type-parameter scopes and instantiation rules
- codegen must choose a concrete implementation strategy
- runtime impact depends on lowering model, but a monomorphized first version
  may avoid runtime support changes

## 8. Interactions

- errors: generic errorable functions must preserve current explicit error rules
- structs: direct interaction for generic structs
- arrays: generic element types would naturally interact with arrays and slices
- control flow: no special interaction
- returns: substitution must preserve return-type validity
- builtins: some builtins may work naturally over type parameters, others may not
- future modules/imports: instantiated exported APIs affect package boundaries
- future richer type features: interfaces and generics are tightly coupled

## 9. Alternatives Considered

- no generics, keep explicit duplication
  - simplest language
  - higher long-term library duplication
- builtin-only expansion instead of generics
  - cheaper now
  - pushes too much policy into the compiler and stdlib
- full constraint-heavy generic system
  - powerful
  - too large for the current language stage

## 10. Complexity Cost

- language surface: high
- parser complexity: moderate
- checker complexity: high
- lowering/codegen complexity: high
- runtime complexity: low to moderate depending on lowering model
- diagnostics complexity: high
- test burden: high
- documentation burden: high

## 11. Why Now?

Generics are already implied by repeated type-specific library patterns. Writing
the proposal now captures the design pressure without committing the language to
an oversized implementation jump.

## 12. Open Questions

- should the first version support generic functions, generic structs, or both?
- should type arguments always be explicit?
- do constraints belong in the first version?
- should code generation monomorphize every instantiation?

## 13. Decision

Exploring. Generics solve real duplication pressure, but the feature is large
enough that it needs a narrower milestone and a clear lowering strategy before
any acceptance.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
