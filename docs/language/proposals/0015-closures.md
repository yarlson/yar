# Proposal: Closures

Status: accepted and implemented

## 1. Summary

Add anonymous function literals and first-class function types with explicit,
capture-by-value closure semantics.

The implemented first version is:

- anonymous function literals
- function types spelled as `fn(T1, T2) R` and `fn(T) !R`
- lexical capture of outer locals by value
- calls through function-valued expressions
- no nested named functions
- no mutation of captured outer bindings inside closure bodies
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

```
fn make_adder(x i32) fn(i32) i32 {
    return fn(y i32) i32 {
        return x + y
    }
}
```

### Invalid examples

```
fn main() i32 {
    f := fn() void {
        return 1
    }
    return 0
}
```

Invalid because the anonymous function body returns a value inconsistent with
its declared return type.

```
fn main() i32 {
    f := fn() i32 {
        return missing
    }
    return 0
}
```

Invalid because closures would not change ordinary name-resolution rules.

## 4. Semantics

Closures create first-class function values that may capture bindings from an
enclosing scope.

The implemented design settles the main questions as follows:

- captures are by value at closure creation time
- captured environments live in runtime-managed heap storage
- nested syntax uses anonymous literals only; there is no nested named `fn`
- function values lower to a code pointer plus environment pointer pair

## 5. Type Rules

- closure literals have function types
- captured names must resolve in lexical scope
- current rules for `return`, `error`, and value types still apply inside the
  closure body
- function values may be assigned, passed, returned, and stored like other
  first-class values
- captured outer locals are readable but not assignable inside closure bodies

## 6. Grammar / Parsing Shape

Implemented syntax:

```
fn(x i32) i32 { return x + 1 }
```

The parser disambiguates function literals from top-level declarations by
position: top-level `fn` still requires a name, while expression-position `fn`
forms parse as function literals.

## 7. Lowering / Implementation Model

- parser adds anonymous function literal syntax
- AST adds closure or function-literal nodes
- checker adds lexical capture analysis and function-type support
- codegen emits synthetic functions plus explicit environment objects
- runtime impact reuses the existing runtime-managed allocation model for
  captured environments

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

- should later versions allow nested named functions?
- should capture-by-reference ever be added explicitly?
- should top-level functions become directly referencable as function values?

## 13. Decision

Accepted and implemented as a small, explicit closure system: function literals
and function types are first-class, captures are by value, and captured outer
bindings remain read-only inside closure bodies.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `current-state.md` update
- [x] `decisions.md` update
