# Proposal: Pointers and Recursive Data

Status: proposed

## 1. Summary

Add a conventional typed pointer model so YAR can represent recursive data.

The minimal surface is:

- pointer types `*T`
- `nil`
- `new(T)` for allocation
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

The prior `box` idea was trying to solve this same problem with a custom
mechanism. After reconsideration, plain typed pointers are the clearer and more
conventional design.

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
    tail := new(Node)
    *tail = Node{value: 2, next: nil}

    head := new(Node)
    *head = Node{value: 1, next: tail}

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

## 4. Semantics

`*T` is an explicit typed pointer to a value of type `T`.

- `new(T)` allocates zero-initialized storage for a `T` and returns `*T`
- `nil` is the zero pointer literal
- `*expr` reads or names the underlying `T` value
- pointers do not support arithmetic, integer conversion, or raw address
  exposure

Recursive data becomes legal only through pointer indirection. Direct recursive
containment remains invalid.

The first version is intentionally minimal. It does not add pointer arithmetic,
unsafe casts, borrowing rules, or implicit dereference.

## 5. Type Rules

- `*T` is valid when `T` is a first-class storable type other than `void` and
  `noreturn`
- `new(T)` has type `*T`
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
- add `nil` as a literal
- extend unary expressions to include prefix dereference `*expr`
- add `new(T)` as a built-in allocation form over a type argument

Examples:

- `*Node`
- `*Expr`
- `new(Node)`
- `(*node).value`
- `if node == nil { ... }`

## 7. Lowering / Implementation Model

- parser: add pointer types, `nil`, dereference, and `new(T)`
- AST: add pointer type representation, `NilLiteral`, and `NewExpr`
- checker: validate pointee types, dereference typing, assignment-target
  legality, pointer comparisons, and recursive-struct exceptions through `*T`
- codegen: lower pointers to LLVM pointer values
- runtime: add allocation support for `new(T)` using the shared runtime-managed
  memory model from proposal `0000-minimal-memory-management.md`

The main implementation cost is memory management. The smallest viable version
is explicit allocation through `new(T)` without exposing manual freeing yet.
That is enough to unblock recursive frontend data structures.

## 8. Interactions

- errors: pointer allocation does not change the explicit error model
- structs: recursive fields become legal through `*T`
- arrays: arrays of pointers become possible
- control flow: no new control-flow rules
- returns: pointers return like other first-class values
- builtins: `new(T)` becomes a fixed language/runtime operation
- future modules/imports: recursive public types become possible once packages
  exist
- future richer type features: enums and tagged unions compose naturally with
  pointer payloads

## 9. Alternatives Considered

### Custom boxed values

Rejected because it invents a bespoke indirection model where ordinary pointers
are easier to understand.

### Integer handles into runtime tables

Rejected because they are less direct, less typed, and harder to explain than
typed pointers.

### Full raw-pointer model with arithmetic and casts

Rejected because frontend self-hosting needs indirection, not unsafe systems
programming surface.

## 10. Complexity Cost

- language surface: medium
- parser complexity: medium
- checker complexity: medium
- lowering/codegen complexity: medium
- runtime complexity: medium to high
- diagnostics complexity: medium
- test burden: medium
- documentation burden: medium

## 11. Why Now?

Recursive data is a prerequisite for a self-hosted frontend. Without pointers,
imports, enums, and slices still do not let YAR model its own AST cleanly.

## 12. Open Questions

- Should YAR also support address-of in the first version, or keep allocation via
  `new(T)` as the only pointer-construction form initially?
- Should pointer equality be limited to `== nil` / `!= nil`, or allow same-type
  pointer comparisons generally?
- Should pointer allocation and reclamation be treated as fully settled by
  proposal `0000-minimal-memory-management.md`, or is any pointer-specific
  clarification still needed?

## 13. Decision

Pending.

Conventional typed pointers appear to be a better fit than a custom `box`
feature for YAR's self-hosting needs.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- runtime allocation support
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
