# Proposal: Generics

Status: accepted

## 1. Summary

YAR now supports a narrow first cut of parametric polymorphism:

- generic structs
- generic functions
- explicit type parameters on declarations
- explicit type arguments at every use site
- monomorphization before semantic checking and code generation

The accepted scope is intentionally small:

- no type-argument inference
- no constraints
- no generic enums
- no generic methods
- no methods on instantiated generic types

## 2. Motivation

The language already had repeated type-specific patterns such as:

- `sort.strings`, `sort.i32s`, and `sort.i64s`
- container-like helpers that differ only by element type
- simple identity/wrapper/value-extraction helpers

Without generics, those patterns either duplicate source or push pressure onto
compiler-owned builtins. A small explicit generic system improves reuse while
preserving YAR's preference for straightforward, predictable compilation.

## 3. User-Facing Examples

### Valid examples

```
struct Box[T] {
    value T
}

fn first[T](values []T) T {
    return values[0]
}

fn wrap[T](value T) Box[T] {
    return Box[T]{value: value}
}

fn main() i32 {
    values := []i32{7, 9}
    box := wrap[i32](first[i32](values))
    return box.value
}
```

### Invalid examples

```
fn id[T](value T) T {
    return value
}

fn main() i32 {
    x := id(1)
    return x
}
```

Invalid because generic function calls require explicit type arguments.

```
struct Box[T] {
    value T
}

fn main() i32 {
    box := Box{value: 1}
    return box.value
}
```

Invalid because generic type uses require explicit type arguments.

```
fn (b Box[i32]) value() i32 {
    return b.value
}
```

Invalid because methods on instantiated generic types are not supported in the
first cut.

## 4. Accepted Semantics

- Type parameters are lexical names scoped to one `struct` or `fn`
  declaration.
- Type arguments are substituted into the declaration at each explicit use
  site.
- The compiler monomorphizes each concrete instantiation into an ordinary
  non-generic declaration before running the checker and code generator.
- Exported generic declarations may be instantiated across package boundaries.
- Only instantiated generic declarations are semantically checked; unused
  generic declarations are still parsed and lowered, but body/type errors that
  depend on substitution are reported only when that instantiation is used.

## 5. Type Rules

- Generic structs and generic functions must declare one or more type
  parameters.
- Each instantiation must provide exactly the declared number of type
  arguments.
- Type arguments must be explicit at every use site.
- Type parameters may appear anywhere a normal type may appear, including in
  arrays, slices, maps, pointers, parameters, returns, locals, and struct
  fields.
- Existing invalid type rules still apply after substitution. For example:
  - `[]void` is still invalid
  - `map[[]i32]T` is still invalid
  - direct recursive containment is still invalid after substitution
- Generic function bodies are checked after substitution using the existing
  non-generic checker.

## 6. Grammar

```
struct Box[T] {
    value T
}

fn first[T](values []T) T {
    return values[0]
}

Box[i32]{value: 1}
first[i32](values)
```

The parser supports:

- type parameter lists after struct and function names
- type argument lists on named type uses
- type argument lists on named function calls
- disambiguation from indexing by requiring generic call syntax to be followed
  by `(` and generic struct literal syntax to be followed by a struct-literal
  `{...}` shape

## 7. Implementation Model

- The parser produces AST nodes for type parameters, type arguments, and
  type-application expressions.
- Package lowering preserves generic declarations while still rewriting
  package-local and imported names to canonical forms.
- A dedicated compiler pass monomorphizes explicit generic instantiations into
  ordinary declarations.
- The checker and code generator continue to operate on a non-generic program.
- No runtime changes are required.

## 8. Interactions

- Errors: generic errorable functions keep the existing `!T`, `?`, and
  `or |err| { ... }` rules after substitution.
- Structs: generic structs compose naturally with ordinary field access and
  keyed literals once instantiated.
- Arrays, slices, maps, and pointers: type parameters may appear inside these
  types and are validated after substitution.
- Imports and exports: exported generic declarations may be instantiated across
  packages, but exported surface rules still apply after substitution.
- Methods: the first cut deliberately excludes generic methods and methods on
  instantiated generic types to avoid coupling generics to receiver lowering.

## 9. Alternatives Considered

- No generics, keep explicit duplication
  - smallest language surface
  - continued library duplication pressure
- Builtin-only expansion
  - cheaper compiler work
  - pushes reusable policy into the compiler instead of user code
- Constraint-heavy generics
  - more expressive
  - too large for the current language stage

## 10. Complexity Cost

- language surface: moderate
- parser complexity: moderate
- lowering complexity: high
- checker/codegen complexity: low to moderate because they still see
  monomorphized code
- runtime complexity: low
- diagnostics complexity: moderate
- test burden: high
- documentation burden: high

## 11. Follow-On Questions

- Should a later version add type-argument inference for obvious local cases?
- Should constraints be introduced before or after any interface work?
- Should generic methods be added, and if so, how should receiver lowering work?
- Should more stdlib packages be rewritten to take advantage of generics?

## 12. Decision

Accepted as a narrow, explicit, monomorphized first cut. This version solves
real duplication pressure without forcing constraints, inference, or runtime
polymorphism into the language yet.

## 13. Implementation Checklist

- parser
- AST updates
- package lowering updates
- monomorphization pass
- checker compatibility updates
- codegen symbol compatibility updates
- diagnostics
- tests
- current-state documentation updates
