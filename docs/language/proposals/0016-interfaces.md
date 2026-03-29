# Proposal: Interfaces

Status: exploring

## 1. Summary

Consider adding interface-style abstraction for behavior-based APIs.

The smallest plausible version would likely include:

- named interface declarations
- explicit method requirements
- assignability when a concrete type satisfies the method set
- no implicit dynamic features beyond interface values themselves

## 2. Motivation

Current YAR favors:

- concrete structs
- explicit package boundaries
- direct functions

That keeps the language readable, but it limits behavior-oriented abstraction.
Once methods or reusable generic code are considered, interface-style contracts
become a natural follow-on question.

Concrete pressure areas could include:

- pluggable formatting or diagnostics sinks
- alternate storage or loading boundaries
- generic algorithms that need behavior, not one concrete type

At the same time, interfaces can easily pull the language toward indirection and
hidden dispatch.

## 3. User-Facing Examples

### Valid examples

```yar
interface Writer {
    write(msg str) !void
}

fn emit(w Writer, msg str) !void {
    return w.write(msg)
}
```

### Invalid examples

```yar
interface Bad {
    value i32
}
```

Invalid because interface members would be behavior requirements, not fields.

```yar
fn emit(w Writer, msg str) !void {
    return w.missing(msg)
}
```

Invalid because interface calls must target declared interface methods.

## 4. Semantics

Interfaces would define behavior contracts rather than data layout.

The design must settle:

- whether satisfaction is implicit or explicit
- whether interface values use dynamic dispatch
- whether nil interface values exist
- whether interfaces belong before or after methods and generics are settled

The smallest coherent design may require methods first, since interfaces without
receiver-based behavior are not very useful.

## 5. Type Rules

- interface declarations contain method requirements
- a concrete type satisfies an interface if it provides the required methods
- interface values can hold only satisfying concrete values
- interface method calls are limited to the declared method set

## 6. Grammar / Parsing Shape

Candidate syntax:

```yar
interface Writer {
    write(msg str) !void
}
```

This adds a new top-level declaration form and method-signature syntax within
interface bodies.

## 7. Lowering / Implementation Model

- parser adds interface declarations
- AST adds interface nodes and possibly interface types
- checker adds satisfaction and assignability rules
- codegen likely needs interface value representations and dynamic dispatch
- runtime may need method tables or equivalent metadata

## 8. Interactions

- errors: interface methods follow ordinary error rules
- structs: structs are likely the main concrete implementers
- arrays: no direct interaction
- control flow: no direct interaction
- returns: interface returns expand abstraction at package boundaries
- builtins: interface values should not blur builtin semantics
- future modules/imports: exported interfaces can become long-lived API contracts
- future richer type features: interfaces are tightly coupled with methods and generics

## 9. Alternatives Considered

- no interfaces, prefer concrete types and package-local small abstractions
  - simplest and most explicit
- explicit adapter structs instead of interface values
  - more verbose
  - keeps dispatch visible
- full implicit interface model
  - powerful
  - high complexity and indirection cost

## 10. Complexity Cost

- language surface: high
- parser complexity: moderate
- checker complexity: high
- lowering/codegen complexity: high
- runtime complexity: moderate to high
- diagnostics complexity: high
- test burden: high
- documentation burden: high

## 11. Why Now?

Interfaces show up quickly whenever methods and generics enter the discussion.
Writing the proposal now helps keep future abstraction work from drifting into
an oversized design by accident.

## 12. Open Questions

- should interfaces wait until methods are accepted?
- should satisfaction be implicit or explicit?
- are interface values worth their runtime cost in YAR?
- can the language stay true to its explicit style with interface dispatch?

## 13. Decision

Exploring. Interfaces are a major abstraction feature and likely depend on
earlier decisions about methods and generics.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
