# Proposal: Interfaces

Status: accepted and implemented

## 1. Summary

YAR supports named interfaces for behavior-oriented APIs.

The implemented design includes:

- named top-level `interface` declarations
- method requirements only inside interface bodies
- implicit satisfaction by concrete receiver types
- interface values with dynamic dispatch
- exact receiver matching consistent with ordinary methods

## 2. Motivation

Methods made it possible to attach behavior to structs, but package boundaries
still forced APIs to name one concrete type at a time. Interfaces provide a
small behavior abstraction layer without introducing hidden exceptions,
operator overloading, or other larger dynamic features.

This keeps the language usable for:

- pluggable sinks and emitters
- package boundaries that should return behavior rather than a concrete type
- hiding private implementation structs behind exported APIs

## 3. User-Facing Examples

### Valid

```
interface Writer {
    write(msg str) !void
}

struct Buffer {
    prefix str
}

fn (b Buffer) write(msg str) !void {
    print(b.prefix + msg)
    return
}

fn emit(w Writer) !void {
    return w.write("ok")
}
```

### Invalid

```
interface Bad {
    value i32
}
```

Invalid because interface bodies contain method requirements only.

```
struct Counter {
    value i32
}

fn (c *Counter) inc() void {
}

fn use(c Counterer) void {
    c.inc()
}
```

Invalid if `Counterer` requires `inc()` on `Counter` values, because receiver
matching stays exact and `Counter` does not satisfy a `*Counter` method set.

## 4. Semantics

- Interfaces are named top-level declarations.
- Interface members are method signatures only.
- Concrete satisfaction is implicit.
- Satisfaction uses the exact concrete receiver type.
- Interface values may hold satisfying concrete values.
- Interface method calls use dynamic dispatch through the interface value.
- Zero-valued interface values are allowed and panic on method call with
  `nil interface method call`.

## 5. Type Rules

- Interface types are first-class named types.
- A concrete type satisfies an interface when it provides every required method
  with matching parameter types, return type, and errorability.
- `T` and `*T` are distinct for both method calls and interface satisfaction.
- Interface-to-interface conversion is supported only for the same exact
  interface type in the current implementation.
- `nil` remains pointer-only and does not coerce to interface types.
- Interface values are not comparable in the current implementation.

## 6. Grammar / Parsing Shape

```
interface Writer {
    write(msg str) !void
}
```

This adds a new top-level declaration form and reuses ordinary parameter and
return-type syntax for interface methods.

## 7. Lowering / Implementation Model

- Parser adds interface declarations and interface method signatures.
- Package lowering rewrites local and imported interface names to canonical
  package-qualified names and enforces export rules.
- Checker records interface metadata, validates satisfaction, and validates
  interface method calls.
- Codegen lowers each interface value to `{data ptr, method-table ptr}`.
- Boxing a concrete value into an interface allocates storage for non-pointer
  concrete values and reuses existing pointers directly for pointer receivers.
- Dynamic dispatch loads the interface method entry from the interface's method
  table and calls an adapter with the boxed data pointer.

## 8. Interactions

- Methods: interfaces depend directly on the existing method model and preserve
  exact receiver matching.
- Errors: interface methods follow the ordinary `!T` and `error` rules.
- Imports: exported interfaces participate in the same `pub` and canonical-name
  rules as exported structs and enums.
- Generics: interfaces are not generic, and there are no interface constraints.
- Closures: unrelated at the surface, but both features use runtime-managed
  heap-backed values in lowering.

## 9. Alternatives Considered

- No interfaces, only concrete types
  - simpler
  - less flexible across package boundaries
- Explicit adapter structs only
  - keeps dispatch fully manual
  - forces more boilerplate into user code
- Richer implicit interface-to-interface conversion
  - more powerful
  - higher semantic and runtime complexity than needed today

## 10. Complexity Cost

- language surface: moderate
- parser complexity: low
- checker complexity: moderate
- lowering/codegen complexity: moderate
- runtime complexity: low to moderate
- diagnostics complexity: moderate
- test burden: moderate
- documentation burden: moderate

## 11. Why Now?

Methods and package work are already in place, so interfaces can now be added
as a focused follow-on abstraction rather than a speculative placeholder.

## 12. Open Questions

- Should interface-to-interface conversion grow beyond exact-type identity?
- Should `nil` eventually become a first-class interface literal?
- Should generic constraints reuse interface syntax or remain separate?

## 13. Decision

Accepted and implemented as a small, explicit interface system:

- named interfaces only
- implicit concrete satisfaction
- dynamic dispatch only through interface values
- exact receiver matching preserved

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- docs/context update
- `docs/YAR.md` update
- `docs/language` update
