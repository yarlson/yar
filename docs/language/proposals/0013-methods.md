# Proposal: Methods

Status: exploring

## 1. Summary

Consider adding method declarations on named types so common operations can be
attached to structs and, if justified later, possibly other user-defined types.

The smallest plausible version is:

- methods on `struct` types only
- value receivers and pointer receivers
- method calls with `value.method(...)`
- no operator overloading or implicit interfaces

## 2. Motivation

The language currently supports:

- structs
- pointers
- package-qualified functions
- explicit exports

That is enough to model data and behavior, but it keeps related operations
outside the type they primarily serve.

Current code has to write:

```yar
pub fn user_name(u User) str {
    return u.name
}
```

instead of a receiver-shaped operation.

This becomes more noticeable as programs grow richer domain models. Methods
could improve API readability and reduce naming repetition, but they also make
name lookup, export rules, and future interface design more complex.

## 3. User-Facing Examples

### Valid examples

```yar
struct User {
    name str
}

fn (u User) display_name() str {
    return u.name
}

fn (u *User) rename(name str) void {
    (*u).name = name
}
```

### Invalid examples

```yar
fn (n i32) double() i32 {
    return n * 2
}
```

Invalid in the smallest version because the first cut would attach methods only
to named struct types.

```yar
fn (u User) rename(name str) void {
    u.name = name
}
```

Invalid if value receivers are immutable copies in the source model.

## 4. Semantics

Methods are declaration sugar for functions with an explicit receiver.

Questions the design must settle:

- whether the receiver participates in ordinary overload resolution
- whether method syntax is only call-site sugar or a distinct declaration kind
- whether pointer receivers require explicit pointer values at call sites
- whether exported methods follow the same `pub` rules as exported functions

The smallest coherent design likely keeps methods as syntax over ordinary
functions during lowering.

## 5. Type Rules

- the receiver type must be a named user-defined type allowed by the proposal
- receiver names are local bindings within the method body
- value receiver methods do not mutate the original caller value unless the
  receiver contains shared indirection
- pointer receiver methods require a pointer-typed receiver

## 6. Grammar / Parsing Shape

One candidate shape:

```yar
fn (u User) display_name() str { ... }
```

Potential ambiguity is low, but the parser must clearly separate:

- ordinary functions
- receiver declarations
- grouped parameters

## 7. Lowering / Implementation Model

- parser adds receiver syntax to function declarations
- AST either stores a receiver field or desugars immediately into an ordinary
  function form
- checker adds method lookup and validates receiver legality
- codegen can likely reuse ordinary function lowering after canonicalization
- runtime impact is likely none

## 8. Interactions

- errors: no special error interaction beyond ordinary functions
- structs: direct interaction; this is primarily a struct feature
- arrays: likely no first-version method support
- control flow: no special interaction
- returns: same as ordinary functions
- builtins: method names must not shadow builtin semantics in confusing ways
- future modules/imports: import and export rules must cover methods clearly
- future richer type features: methods strongly influence interface and generic
  design, so this feature should not land casually

## 9. Alternatives Considered

- stay with free functions only
  - simplest model
  - keeps lookup and API shape explicit
- add methods only as call-site sugar over ordinary functions
  - lowers complexity
  - may feel inconsistent if declarations do not visibly belong to the type
- add full method sets immediately
  - likely too large for the current language stage

## 10. Complexity Cost

- language surface: moderate increase
- parser complexity: low to moderate
- checker complexity: moderate
- lowering/codegen complexity: low if desugared cleanly
- runtime complexity: none
- diagnostics complexity: moderate because receiver lookup errors must stay clear
- test burden: moderate
- documentation burden: moderate

## 11. Why Now?

Methods are a recurring next-step feature whenever richer data modeling comes
up. Writing the proposal now makes the tradeoffs explicit before ad hoc syntax
appears in examples or downstream ideas.

## 12. Open Questions

- should methods be allowed only on structs in the first version?
- should value receiver mutation be rejected explicitly?
- how should exported methods be spelled and documented?
- should methods remain pure sugar over functions in the internal model?

## 13. Decision

Exploring. Methods are plausible, but the language does not yet have a settled
receiver model or a clear milestone that requires them.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
