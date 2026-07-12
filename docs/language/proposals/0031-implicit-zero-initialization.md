# Proposal: Package-Relative Implicit Zero Initialization

Status: accepted

Implementation: implemented

## 1. Summary

Check every source construct that synthesizes a value without an expression.
Initializer-free locals, omitted struct fields, and omitted fixed-array tails
are valid only when the synthesized type has an implicit zero value accessible
in the package where the construct appears.

The rule is recursive for arrays and structs and uses origin-safe package
ownership for private fields.

## 2. Motivation

Field visibility prevents an importing package from spelling a literal for a
struct with private fields, but implicit initialization could still construct
the same representation through `var value imported.Type`, a containing array
or wrapper, or an omitted aggregate slot. The old behavior also admitted null
maps and function values, unnamed zero errors, and implicit enum cases despite
having no coherent source-level contract for them.

One type predicate applied to every omitted value closes all nested forms
without removing useful primitive, nil, empty, or owner-package zero values.

## 3. Source Constructs

The compiler checks implicit zero eligibility for:

```yar
var value T
Record{provided: value}
[4]T{first, second}
```

The first form synthesizes one `T`. The struct literal synthesizes every omitted
field. The array literal synthesizes its omitted tail.

Explicit values do not require implicit zero eligibility:

```yar
var value T = make_t()
Record{field: make_t()}
[2]T{make_t(), make_t()}
```

## 4. Leaf Types

These types are implicitly zeroable:

| Type | Implicit zero |
| --- | --- |
| `bool` | `false` |
| `i32`, `i64` | `0` |
| `str` | empty string |
| `*T` | `nil`, without constructing `T` |
| `[]T` | empty nil-backed slice, without constructing `T` |
| interface | nil interface |
| `chan[T]` | closed nil channel, without constructing `T` |

Calling a method on a zero interface retains the deterministic
`nil interface method call` trap. Sending to or receiving from a zero channel
returns `error.Closed`; closing it is a no-op.

These types require an explicit value:

- `map[K]V`, using at least `map[K]V{}`;
- function values, using a function or function literal;
- `error`, using a declared error identity;
- enums, using an explicit case.

Errorable `!T` values also have no implicit zero. They are produced by language
operations and must be handled or propagated before ordinary local binding.

`void` and `noreturn` remain invalid local and aggregate element types.

## 5. Arrays

`[0]T` is implicitly zeroable because it contains no element. `[N]T` for
positive `N` is implicitly zeroable exactly when `T` is.

An array literal may contain a non-zeroable element type when all elements are
provided. If it omits a tail, the element type must be implicitly zeroable at
that use site.

## 6. Structs and Package Ownership

A struct is implicitly zeroable in package `P` exactly when:

1. every field type is recursively implicitly zeroable in `P`; and
2. every private field belongs to `P`.

Consequences:

- a package may zero its own private or resource structs when their fields are
  zeroable;
- an importing package cannot zero a struct with private fields;
- an imported all-public struct remains zeroable when its field types are;
- a local transparent wrapper cannot hide an implicit zero of another
  package's private-field struct;
- pointers, slices, interfaces, and channels containing or referring to an
  opaque type remain zeroable because their zero does not construct that type.

The existing struct-literal ownership rule remains: any private field blocks
the entire literal outside the declaring package. Inside an allowed literal,
each omitted field independently requires an accessible implicit zero.

Private fields express package authority. This proposal does not add a stronger
constructor-only invariant inside the owning package.

## 7. Generic Structs

Generic structs apply the rule to their instantiated field types. Private-field
ownership remains with the generic declaration package, not the instantiating
package. A type argument that is not represented by any field does not affect
zeroability.

## 8. Enums and Other Aggregates

Enums have no implicit zero case. Source code must choose a case explicitly.
When an enum payload syntax omits payload fields, each omitted field follows the
same implicit-zero rule.

Empty slice and map literals are explicit constructors. Unused slice capacity,
channel buffer storage, struct padding, ignored success slots in error results,
and runtime ABI output storage are not source values and are outside this rule.

## 9. Diagnostics

Diagnostics name the source type and first blocking nested type without exposing
internal canonical package prefixes. Representative forms are:

- `local "value" requires an initializer because type "dep.Secret" has no accessible zero value`
- `field "secret" must be initialized because type "dep.Secret" has no accessible zero value`
- `array literal must initialize all elements because type "dep.Secret" has no accessible zero value`

## 10. Compatibility

Primitive declarations, nil pointers, empty slices, nil interfaces, closed nil
channels, local primitive-field structs, and accessible transparent structs
retain initializer-free declarations. Code that relied on implicit maps,
functions, errors, enums, imported private-field structs, or nested omitted
values must provide an explicit value. Errorable values must instead be handled
or propagated through the existing error flow.

The change is compile-time only. Runtime layouts and ABIs do not change.

## 11. Alternatives Considered

### Ban every initializer-free declaration

This is mechanically simple but removes useful and well-defined scalar, nil,
empty, and owner-package zero values.

### Restrict only imported private-field structs

This leaves the same construction available through arrays and wrappers and
retains incoherent null maps, null functions, unnamed errors, and implicit enum
cases.

### Ban private and resource struct zeros even in the owner

The owner may already initialize or omit its private fields. A constructor-only
invariant inside the owner would require a separate nominal invariant feature,
not field visibility.

### Remove zero interfaces

Nil interfaces already have deterministic behavior and do not construct a
hidden concrete value. Removing them adds churn without strengthening package
ownership.

## 12. Decision

Accepted. Source-level implicit zero synthesis is a package-relative recursive
type capability. Useful scalar, nil, empty, and owner-package zeros remain;
types without a coherent implicit source value require explicit initialization.

## 13. Acceptance and Implementation Checklist

- [x] validate initializer-free locals
- [x] validate omitted struct fields and fixed-array tails
- [x] define scalar, pointer, slice, interface, and channel zeros
- [x] require explicit map, function, error, and enum values
- [x] reject implicit zeros for errorable values
- [x] recurse through arrays and struct fields
- [x] enforce origin-safe private-field ownership
- [x] preserve owner-package private and resource struct zeros
- [x] apply instantiated generic field types and declaration ownership
- [x] keep explicit aggregate values independent of zero eligibility
- [x] preserve runtime layouts and ABIs
- [x] cover direct, nested, generic, package, and stdlib behavior
- [x] update current public, internal, derived, and design documentation
