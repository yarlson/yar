# Proposal: Struct Field Visibility and Package-Owned Construction

Status: accepted
Implementation: implemented

## 1. Summary

Make struct fields package-private by default and add `pub field Type` for fields
that form part of an exported representation. External selector operations are
allowed only for public fields. If a struct has any private field, struct-literal
construction belongs to its declaring package.

This provides user-defined opaque and mixed-visibility structs without an
`opaque` keyword or stdlib-specific compiler metadata.

## 2. Motivation

Before this change, ordinary imported struct fields were all externally
accessible and constructible. The compiler separately hardcoded `fs.File`,
`net.Conn`, and `net.Listener` as opaque. User packages could not protect
invariants or hide representation, while equivalent stdlib behavior depended on
type names known by the package loader.

Field visibility should follow package ownership uniformly. A public type should
be able to expose methods and selected data without exposing every internal
field or allowing callers to forge package-owned state.

## 3. User-Facing Examples

Public and private fields may be mixed:

```yar
package account

struct secretState {
    attempts i32
}

pub struct User {
    pub name str
    state secretState
}

pub fn new_user(name str) User {
    return User{name: name, state: secretState{attempts: 0}}
}

pub fn (u User) attempts() i32 {
    return u.state.attempts
}
```

External code may use the public field and exported methods:

```yar
user := account.new_user("Ada")
print(user.name)
user.name = "Grace"
print(to_str(user.attempts()))
```

External access to the private field is invalid:

```yar
print(to_str(user.state.attempts))
// checker: field "state" of struct "account.User" is not exported by package "account"
```

Because `User` has a private field, all external literals are invalid, including
ones that mention only public fields:

```yar
forged := account.User{name: "forged"}
// checker: struct literal for "account.User" is not allowed outside package
// "account" because it has package-private fields
```

A fully public representation remains externally constructible:

```yar
package geometry

pub struct Point {
    pub x i32
    pub y i32
}
```

```yar
p := geometry.Point{x: 1, y: 2}
p.x += 1
```

## 4. Semantics

- A struct field without `pub` is visible only inside the struct's declaring
  package.
- A `pub` field is visible wherever its containing struct type is visible.
- Same-package functions and methods may read, assign, compound-assign, take the
  address of, and initialize every field.
- Function literals retain the package authority of their defining function, so
  a closure defined in the owning package may use that package's private fields.
- Outside the declaring package, those selector operations require a public
  field.
- If any field is private, all struct literals for that type are rejected
  outside the declaring package. This includes empty literals, literals naming
  only public fields, and address-of composite literals.
- A struct with no private fields retains ordinary external literal construction.
- A public struct with only private fields is an opaque package-owned type.
- There is no `opaque` declaration modifier.
- Package ownership uses the declaration's resolved origin-safe package identity,
  not the import spelling or displayed package name.

The construction restriction does not yet cover zero-value declarations or
aggregate zero-initialization. For example, `var value imported.Type` remains
accepted when `Type` has private fields because it does not use a struct literal.
Closing those loopholes belongs to the separate zero-value/initialization design
work.

## 5. Type Rules

- Public fields of exported structs are part of the exported API and cannot use
  non-exported local struct, interface, or enum types, including through nested
  type forms and generic arguments.
- Private fields are not part of the exported API and may use non-exported local
  types.
- Existing exported function, method, interface, and enum signature validation
  remains unchanged.
- Generic structs declare visibility once; every monomorphized instantiation
  preserves it.
- The generic declaration's package owns private fields and construction, not
  the package that requests an instantiation.
- Enum payload field grammar and visibility are unchanged. Payload fields remain
  inherently public. `pub` produces the targeted parse diagnostic
  `enum payload fields are inherently public and do not accept 'pub'`.
- A `pub` field inside a non-exported struct has no cross-package effect because
  the containing type itself is unavailable externally.

## 6. Grammar / Parsing Shape

Struct fields accept an optional `pub` prefix:

```text
StructField = [ "pub" ] identifier Type .
```

The prefix is parsed only in struct bodies. Enum payload fields keep their
existing grammar.

## 7. Lowering / Implementation Model

- The AST stores one visibility flag on each struct field.
- Package lowering preserves the flag and validates private-type exposure only
  for public fields.
- Monomorphization copies visibility into each concrete struct declaration.
- Checker struct metadata carries field visibility. Selector and addressability
  checks compare the current function package with the struct owner.
- Struct-literal checking rejects external construction when any field is
  private.
- Code generation consumes already-checked layout metadata and needs no runtime
  visibility mechanism.
- The package loader no longer assigns an `opaque` bit to selected stdlib type
  names. `fs.File`, `net.Conn`, `net.Listener`, `process.Limits`,
  `process.Cancellation`, and `testing.T` use ordinary private fields.

## 8. Interactions

- Public fields remain mutable and addressable; this proposal does not add
  read-only fields or properties.
- Exported methods provide behavior for package-owned representations and retain
  same-package access to private fields.
- Pointer-to-struct implicit dereference follows the same visibility rule as
  direct selection.
- The generated test runner constructs private `testing.T` state with
  `testing.new` and reads status/messages through methods.
- Transparent stdlib data uses explicit public fields: `fs.DirEntry`,
  `net.Addr`, and `process.Result`.
- Resource and concurrency classifications are independent of field visibility.
- Enum payloads remain compatible with existing keyed and single-field
  positional constructors.

## 9. Alternatives Considered

### Keep all ordinary fields public and hardcode opaque stdlib types

Rejected because user packages cannot enforce the same invariants and compiler
behavior depends on specific package/type names.

### Add an `opaque struct` modifier

Rejected because struct-wide opacity cannot express mixed public/private
representations and creates a second visibility mechanism.

### Allow external literals to initialize only public fields

Rejected because omitted private fields currently receive zero values, allowing
external code to forge package-owned state.

### Make enum payload fields private by default too

Deferred. It would change constructor and match-payload semantics beyond the
struct representation problem addressed here.

## 10. Complexity Cost

- language surface: low; one existing keyword is reused in struct bodies
- parser and AST: low
- package lowering and checker: moderate
- code generation and runtime: none
- migration: explicit `pub` on fields intentionally consumed across packages
- testing and documentation: moderate

## 11. Why Now?

The stdlib already requires package-owned resource representations, and user
packages need the same boundary. Removing type-name hardcoding also makes future
stdlib and dependency types follow one language rule.

## 12. Open Questions

None for this accepted scope. Zero-value construction and enum payload
visibility remain explicitly separate.

## 13. Decision

Accepted. Field-level visibility is the smallest general mechanism that supports
transparent, mixed, and opaque structs while keeping package ownership explicit.

## 14. Acceptance and Implementation Checklist

- [x] parse and retain optional `pub` on struct fields
- [x] preserve visibility through package lowering and generic monomorphization
- [x] enforce selector and addressability visibility across packages
- [x] make any private field block external struct-literal construction
- [x] allow private fields to use private types while rejecting public exposure
- [x] remove stdlib type-name-based opacity metadata
- [x] migrate transparent fields and package-owned stdlib APIs
- [x] cover public, private, mixed, generic, same-package, and stdlib behavior
- [x] keep enum payload field syntax and semantics unchanged
- [x] update current public, internal, derived, and design documentation
