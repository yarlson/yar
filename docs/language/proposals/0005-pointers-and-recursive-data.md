# Proposal: Pointers and Recursive Data

Status: proposed

## 1. Summary

Add a conventional typed pointer model so YAR can represent recursive data.

The minimal surface is:

- pointer types `*T`
- unary `&expr` for address-of
- `nil`
- unary `*expr` for dereference
- pointer equality against `nil` and same-typed pointers

## 2. Motivation

Current YAR structs are value-only and recursive struct containment is rejected.
That keeps the language simple, but it makes recursive trees and graphs
impossible.

Frontend self-hosting creates direct pressure here:

- an AST needs recursive expressions, statements, and blocks
- linked and tree-like helper structures become natural
- enums alone are not enough without a way to point to nested data

## 3. User-Facing Examples

### Valid examples

```yar
struct Node {
    value i32
    next *Node
}

fn set_value(node *Node, value i32) void {
    (*node).value = value
}

fn main() i32 {
    tail := &Node{value: 2, next: nil}
    head := &Node{value: 1, next: tail}

    set_value(head, 3)
    return (*head).value
}
```

### Invalid examples

```yar
struct Bad {
    next Bad
}
```

Invalid because direct recursive containment is still not allowed.

```yar
var p *void
```

Invalid because `void` is not a storable pointee type.

```yar
x := *1
```

Invalid because dereference requires a pointer operand.

```yar
x := &(1 + 2)
```

Invalid because address-of requires an addressable operand or a composite
literal.

## 4. Semantics

`*T` is an explicit typed pointer to a value of type `T`.

- `&expr` returns a `*T` when `expr` is an addressable `T` value
- `&Type{...}` returns a `*Type` for a fresh storage location initialized from
  the composite literal
- `nil` is the zero pointer literal
- `*expr` reads or names the underlying `T` value
- pointers do not support arithmetic, integer conversion, or raw address
  exposure

Recursive data becomes legal only through pointer indirection. Direct recursive
containment remains invalid.

The first version is intentionally minimal. It does not add pointer arithmetic,
unsafe casts, borrowing rules, or implicit dereference.

Address-of uses familiar addressability rules:

- locals and parameters are addressable
- struct fields are addressable when their base expression is addressable
- array and slice elements are addressable when their base expression is
  addressable
- `*expr` is addressable when `expr` has type `*T`
- composite literals are allowed as a special case for fresh storage
- calls, arithmetic expressions, comparisons, and other temporary values are not
  addressable

The storage behind an address-taken value is an implementation detail as long as
source semantics hold. The compiler may keep non-escaping address-taken values
in ordinary local storage and may move escaping ones into runtime-managed
storage.

## 5. Type Rules

- `*T` is valid when `T` is a first-class storable type other than `void` and
  `noreturn`
- `&expr` requires `expr` to be addressable and have type `T`
- `&expr` has type `*T`
- `nil` is assignable only to pointer types
- `*expr` requires `expr` to have type `*T`
- `*expr` has type `T`
- `*expr` may appear in assignment-target position
- `==` and `!=` are valid for two operands of the same pointer type, and for a
  pointer compared with `nil`
- raw errorable expressions cannot be dereferenced directly; they must first be
  handled with existing error forms

## 6. Grammar / Parsing Shape

- extend type parsing to support prefix `*T`
- extend unary expressions to include prefix address-of `&expr`
- add `nil` as a literal
- extend unary expressions to include prefix dereference `*expr`

Examples:

- `*Node`
- `*Expr`
- `&node`
- `&Node{value: 1}`
- `(*node).value`
- `if node == nil { ... }`

## 7. Lowering / Implementation Model

- parser: add pointer types, `nil`, address-of, and dereference
- AST: add pointer type representation, `NilLiteral`, and `AddressOfExpr`
- checker: validate pointee types, dereference typing, assignment-target
  legality, addressability, pointer comparisons, and recursive-struct
  exceptions through `*T`
- codegen: lower pointers to LLVM pointer values
- runtime: support runtime-managed storage for escaping address-taken values and
  composite-literal pointer construction using the shared memory model from
  proposal `0000-minimal-memory-management.md`

The main implementation cost is memory management. The smallest viable version
is address-of plus fresh storage for composite literals without exposing manual
freeing. That is enough to unblock recursive frontend data structures.

## 8. Interactions

- errors: pointer allocation does not change the explicit error model
- structs: recursive fields become legal through `*T`
- arrays: arrays of pointers become possible
- control flow: no new control-flow rules
- returns: pointers return like other first-class values
- future modules/imports: recursive public types become possible once packages
  exist
- future richer type features: enums and tagged unions compose naturally with
  pointer payloads

## 9. Complexity Cost

- language surface: medium
- parser complexity: medium
- checker complexity: medium
- lowering/codegen complexity: medium
- runtime complexity: medium to high
- diagnostics complexity: medium
- test burden: medium
- documentation burden: medium

## 10. Why Now?

Recursive data is a prerequisite for a self-hosted frontend. Without pointers,
imports, enums, and slices still do not let YAR model its own AST cleanly.

## 11. Open Questions

- Should the language allow taking the address of slice elements in the first
  version, or postpone that if it complicates later storage-movement rules?
- Should pointer equality be limited to `== nil` / `!= nil`, or allow same-type
  pointer comparisons generally?
- Should pointer storage and reclamation be treated as fully settled by proposal
  `0000-minimal-memory-management.md`, or is any pointer-specific clarification
  still needed?

## 12. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- runtime allocation support
- diagnostics
- tests
- `current-state.md` update
