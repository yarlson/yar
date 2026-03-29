# Proposal: Methods

Status: accepted

## 1. Summary

Method declarations on named struct types let common operations live next to
the data they primarily serve.

The implemented first version is:

- methods on `struct` types only
- value receivers and pointer receivers
- method calls with `value.method(...)`
- exact receiver-type matching at call sites
- no method values
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

Invalid in the implemented version because the first cut attaches methods only
to named struct types.

```yar
counter := &Counter{value: 1}
counter.current()
```

Invalid when `current` is declared on `Counter` rather than `*Counter`, because
this version does not insert implicit `&` or `*` conversions for method calls.

## 4. Semantics

Methods are declaration sugar for functions with an explicit receiver.

The first version settles the main design questions as follows:

- the receiver is part of method lookup, not ordinary function overload resolution
- exported methods use the same `pub` spelling as exported functions
- pointer receivers require an explicit pointer-typed receiver value at the call site
- methods are not first-class values in this version
- lowering keeps methods as ordinary functions plus an explicit receiver argument

## 5. Type Rules

- the receiver type must be a named local struct type or a pointer to one
- receiver names are local bindings within the method body
- value receiver methods receive a copy of the caller value
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
- checker adds method lookup, export checks, and receiver legality validation
- codegen reuses ordinary function lowering and prepends the receiver argument
- runtime impact is likely none

## 8. Interactions

- errors: no special error interaction beyond ordinary functions
- structs: direct interaction; this is primarily a struct feature
- arrays: likely no first-version method support
- control flow: no special interaction
- returns: same as ordinary functions
- builtins: method names must not shadow builtin semantics in confusing ways
- future modules/imports: exported methods are callable on imported exported
  struct values; non-exported methods stay package-local
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

- should a later version add implicit address-of or dereference for method calls?
- should methods remain struct-only once interfaces or generics arrive?
- should method values or closures be introduced later?

## 13. Decision

Accepted and implemented as a small, explicit feature: methods exist only on
named struct types, use exact receiver matching, and lower to ordinary
functions.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `current-state.md` update
- [x] `decisions.md` update
