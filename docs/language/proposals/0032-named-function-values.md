# Proposal: Named Functions as Values

Status: accepted

Implementation: implemented

## 1. Summary

Allow a non-generic top-level function to be used wherever a function value
is expected. `apply(values, double)` and `http.route("GET", "/users",
list_users)` pass the named function directly instead of wrapping it in a
forwarding literal.

## 2. Motivation

Function types, closures, and higher-order stdlib APIs already exist, but only
anonymous literals produced function values. Route tables, callbacks, and
transforms therefore repeated the full signature at every use:

```yar
http.route("GET", "/users", fn(req http.Request) !http.Response {
    return list_users(req)
})?
```

The pressure point is the HTTP router from proposal 0034: route tables are
long lists of named handlers, and the wrapper adds a signature that can drift
from the handler it forwards to.

## 3. User-Facing Examples

Valid:

```yar
fn double(value i32) i32 {
    return value * 2
}

fn main() i32 {
    transform := double
    upper := strings.to_upper
    return apply([]i32{1, 2, 3}, transform) + len(upper("ok"))
}
```

Invalid:

```yar
fn pick[T](value T) T { return value }
fn stop() noreturn { panic("stop") }

fn main() i32 {
    generic := pick      // generic function "pick" cannot be used as a value
    halt := stop         // noreturn function "stop" cannot be used as a value
    hidden := dep.hidden // package "dep" does not export function "hidden"
    return 0
}
```

## 4. Semantics

- A bare identifier that names a top-level function in the current package,
  and is not shadowed by a local, parameter, match binding, or `or` binding, is
  a function value.
- `pkg.name` is a function value when `pkg` is an import qualifier that is not
  shadowed and `name` is an exported top-level function of that package.
- The value's type is the function's declared type, such as `fn(i32) i32` or
  `fn(str) !void`.
- Locals win over functions with the same name, matching call resolution.
- Calls are unchanged: `name(args)` still calls the function directly.
- Methods remain non-first-class. Generic functions need explicit
  instantiation that the language does not yet support in value position.
  `noreturn` functions cannot be values because function types cannot express
  `noreturn` results. The entry package's `main` is not a value.

## 5. Lowering / Implementation Model

Package lowering rewrites each function-value reference into a function
literal that forwards its parameters to the canonical function name:

```yar
double
// lowers to
fn(value i32) i32 { return main.double(value) }
```

`void` functions forward with an expression statement. The literal captures
nothing, so later passes (checker, monomorphization, closure conversion, and
codegen) see only ordinary closures and need no new representation. Canonical
function names contain `.`, so forwarded calls cannot be shadowed by the
copied parameter names.

## 6. Interactions

- Errors: errorable functions become errorable function values.
- Generics: generic functions are rejected; generic call sites that receive a
  function value work unchanged.
- Closures: the produced literal has no captures.
- Concurrency: function values remain non-share-safe, so they still cannot
  cross `spawn`. `spawn name(args)` keeps its existing direct-call form.
- Packages: exported-function visibility is enforced at the reference site.

## 7. Alternatives Considered

### A dedicated function-pointer representation

Emitting direct function pointers avoids one indirect call but needs a new
value kind in the checker and codegen next to closures. The forwarding literal
reuses the closure path completely.

### Keep literals only

This keeps the language smaller but makes callback-heavy APIs, especially
route tables, noisy and error-prone.

## 8. Tests

- `testdata/function_values`: local, stdlib, `void`, errorable, and falling-off
  `!void` function values; local shadowing.
- `lower::tests::rejects_function_values_that_cannot_be_forwarded`: private,
  generic, and `noreturn` references.
- `testdata/stdlib_http_router`: a named handler passed to `http.route`.

## 9. Decision

Accepted. Named non-generic functions are values of their declared function
type, lowered to capture-free forwarding literals.

## 10. Implementation Checklist

- [x] lower unshadowed local and imported function references
- [x] reject private, generic, and `noreturn` references with direct diagnostics
- [x] runtime fixture and lowering tests
- [x] `docs/YAR.md`, `LLM.txt`, and context docs
