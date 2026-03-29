# Proposal: Closures

Status: exploring

## 1. Summary

Consider adding nested functions that can capture surrounding locals.

The smallest plausible version is:

- anonymous function literals
- lexical capture of locals
- no mutation of captured-by-value bindings unless the model is explicit
- no hidden async or deferred execution semantics

## 2. Motivation

Current YAR has:

- top-level functions
- explicit control flow
- pointers and heap-backed storage

But it does not have a way to define short-lived behavior inline or return a
function that retains local context.

That limits:

- callback-style APIs if they appear later
- local helper abstraction without promoting everything to top-level package scope
- deferred design space around function values

Closures could improve local expressiveness, but they also create a major shift
in the runtime and memory model.

## 3. User-Facing Examples

### Valid examples

```yar
fn make_adder(x i32) fn(i32) i32 {
    return fn(y i32) i32 {
        return x + y
    }
}
```

### Invalid examples

```yar
fn main() i32 {
    f := fn() void {
        return 1
    }
    return 0
}
```

Invalid because the anonymous function body returns a value inconsistent with
its declared return type.

```yar
fn main() i32 {
    f := fn() i32 {
        return missing
    }
    return 0
}
```

Invalid because closures would not change ordinary name-resolution rules.

## 4. Semantics

Closures would create function values that may capture bindings from enclosing
scope.

The design must settle:

- capture by value, by reference, or a mixed rule
- lifetime of captured locals after the outer function returns
- whether nested named functions are also allowed
- how function types are spelled in the language

This feature has direct pressure on the runtime-managed memory model because
captured environments must remain valid after the outer scope exits.

## 5. Type Rules

- closure literals have function types
- captured names must resolve in lexical scope
- current rules for `return`, `error`, and value types still apply inside the
  closure body
- if closures are first-class values, assignment, parameter, and return typing
  must support function types

## 6. Grammar / Parsing Shape

Candidate syntax:

```yar
fn(x i32) i32 { return x + 1 }
```

The parser must disambiguate function literals from top-level declarations and
from grouped expressions.

## 7. Lowering / Implementation Model

- parser adds anonymous function literal syntax
- AST adds closure or function-literal nodes
- checker adds lexical capture analysis and function-type support
- codegen likely needs environment objects plus callable code pointers, or a
  stricter desugaring model
- runtime likely needs a representation for closure environments

## 8. Interactions

- errors: closures must preserve explicit errorable function rules
- structs: closures may become fields if function types are first-class
- arrays: no direct interaction, but container support for function values may matter
- control flow: captured variables can obscure mutation and lifetime if the model is loose
- returns: returning closures is one of the main pressure cases
- builtins: none directly
- future modules/imports: exported function-valued APIs would expand package surface complexity
- future richer type features: closures strongly interact with interfaces, generics, and concurrency

## 9. Alternatives Considered

- no closures, keep only top-level functions
  - simplest model
  - forces helper logic into package scope
- nested functions without capture
  - cheaper than full closures
  - may still solve some local organization needs
- full first-class closure system
  - flexible
  - high implementation and runtime cost

## 10. Complexity Cost

- language surface: high
- parser complexity: moderate
- checker complexity: high
- lowering/codegen complexity: high
- runtime complexity: high
- diagnostics complexity: high
- test burden: high
- documentation burden: high

## 11. Why Now?

Closures are a common next question once function abstraction grows beyond
top-level declarations. Capturing the design space now helps keep later API and
runtime decisions honest.

## 12. Open Questions

- does the first version need capture at all, or only nested functions?
- how should function types be spelled?
- should captures be immutable by default?
- can the current runtime model support closure environments cleanly?

## 13. Decision

Exploring. Closures are valuable but interaction-heavy. They need a much
clearer runtime and type-story before they fit an implementation milestone.

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
