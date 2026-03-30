# Proposal: Testing Framework

Status: accepted

## 1. Summary

Add a `yar test` CLI command and a `testing` stdlib package that together
provide built-in test discovery, execution, and assertion support.

The implemented version provides:

- test file discovery via `_test.yar` suffix
- test function discovery via `test_*` naming convention
- required signature: `fn test_*(t *testing.T) void`
- synthetic test runner generation that replaces user `main()`
- `testing.T` struct with failure tracking and message accumulation
- generic assertion helpers: `testing.equal[V]`, `testing.not_equal[V]`,
  `testing.is_true`, `testing.is_false`
- pass/fail reporting with exit code indication

## 2. Motivation

Without built-in testing support, verifying language behavior required
hand-written test programs with manual assertion logic and no standard
reporting. Each test program needed its own `main()`, its own comparison code,
and its own output format.

A testing framework provides:

- a standard way to write, discover, and run tests
- consistent assertion and reporting conventions
- separation of test code from production code via file naming
- a single command (`yar test`) that handles the full lifecycle

This is especially important for a language that aims to be self-hosting, where
a reliable test harness is needed to validate compiler and stdlib changes.

## 3. User-Facing Examples

### Valid examples

```
// math_test.yar
import "testing"

fn test_addition(t *testing.T) void {
    testing.equal[i32](t, 1 + 1, 2)
}

fn test_string_concat(t *testing.T) void {
    result := "hello" + " " + "world"
    testing.equal[str](t, result, "hello world")
}
```

```
$ yar test ./math
PASS: test_addition
PASS: test_string_concat

2 passed, 0 failed
```

### Invalid examples

```
fn test_bad() void {
    // ...
}
```

Invalid because test functions must accept `*testing.T` as their only
parameter.

```
fn test_bad(t *testing.T) i32 {
    return 0
}
```

Invalid because test functions must return `void`.

```
fn (s *Suite) test_method(t *testing.T) void {
    // ...
}
```

Invalid because test functions must not have receivers.

```
fn test_generic[T](t *testing.T) void {
    // ...
}
```

Invalid because test functions must not have type parameters.

## 4. Semantics

- test files are files whose name ends with `_test.yar`
- test files are excluded from normal `build` and `run` compilations
- test functions are functions in test files whose name starts with `test_`
- test functions must have exactly one parameter of type `*testing.T` and
  return `void`
- test functions must not have receivers or type parameters
- the `yar test` command discovers all test functions in the target package,
  generates a synthetic `main()` that calls each one, and replaces any
  user-defined `main()`
- each test receives a fresh `*testing.T` value
- `testing.T` tracks a `failed` flag and accumulated `messages`
- after each test function returns, the runner checks the `failed` flag and
  prints `PASS: <name>` or `FAIL: <name>` with any accumulated messages
- after all tests run, the runner prints a summary line
  (`"<N> passed, <N> failed"`) and exits with code 1 if any test failed
- test execution only works on native targets (cross-compiled test binaries
  cannot be run directly)

## 5. Type Rules

- test function parameter must be exactly `*testing.T`
- test function return type must be exactly `void`
- `testing.equal[V]` and `testing.not_equal[V]` require `V` to support `==`
  comparison and `to_str` conversion
- `testing.is_true` and `testing.is_false` require a `bool` argument

## 6. Grammar / Parsing Shape

No new syntax. Test discovery is file-name and function-name based. The
`testing` package uses existing language features: structs, methods, generics,
pointers, and builtins.

The synthetic test runner is generated as valid Yar source that is added to the
package before compilation.

## 7. Lowering / Implementation Model

- parser impact: none
- AST / IR impact: none; the generated runner is parsed through the normal
  pipeline
- checker impact: none beyond normal type checking of the generated code
- codegen impact: none beyond normal code generation
- compiler orchestration impact: high; `CompileTestPath` adds test discovery,
  synthetic main generation, user main removal, and testing package import
  registration
- runtime impact: none

The synthetic runner generation:

1. loads the package graph with test files included
2. discovers test functions by scanning for `test_*` with correct signature
3. generates a Yar source string containing a `main()` that creates a
   `testing.T` for each test, calls the test function, checks the `failed`
   flag, and prints results
4. removes any existing user `main()` from the entry package
5. adds the generated source as a file in the entry package
6. ensures the `testing` package import is registered
7. compiles through the standard pipeline

## 8. Interactions

- errors: test functions can test error-returning functions using `or |err|`
  and `testing.equal[error]`
- structs: `testing.T` is a regular struct with methods
- arrays: no special interaction
- control flow: test functions run sequentially; no concurrent test execution
- returns: test functions return `void`; the runner checks `t.failed` after
  return
- builtins: the generated runner uses `to_str` for integer-to-string
  conversion in summary output; `testing.equal[V]` uses `to_str` for failure
  messages
- future modules/imports: test files import `testing` through the normal import
  mechanism
- future richer type features: no special interaction

## 9. Alternatives Considered

- require users to write their own test harnesses
  - simplest for the compiler
  - poor developer experience; no standard conventions
- use a `#[test]` attribute on functions
  - requires new syntax and attribute system
  - overkill for the current language stage
- run each test file as a separate binary
  - simpler runner but slower execution and no shared package state

## 10. Complexity Cost

- language surface: none (no new syntax)
- parser complexity: none
- checker complexity: none
- lowering/codegen complexity: none
- compiler orchestration complexity: high (test discovery, synthetic main
  generation, user main replacement)
- runtime complexity: none
- diagnostics complexity: low
- test burden: moderate (meta-testing: tests that test the test framework)
- documentation burden: moderate

## 11. Why Now?

The language had reached a point where stdlib packages and compiler behavior
needed systematic testing. Without a test framework, validating changes
required manual test programs. Building the testing infrastructure now
establishes conventions early and supports ongoing development toward
self-hosting.

## 12. Open Questions

- should test functions support subtests or test grouping in a future version?
- should `yar test` support filtering by test name?
- should test execution support parallelism?
- should there be a benchmark or timing facility?

## 13. Decision

Accepted and implemented. The `yar test` command discovers `test_*` functions
in `_test.yar` files, generates a synthetic runner, and reports pass/fail
results. The `testing` stdlib package provides `T` with failure tracking and
generic assertion helpers.

## 14. Implementation Checklist

- [x] parser
- [x] AST / IR updates
- [x] checker
- [x] codegen
- [x] diagnostics
- [x] tests
- [x] `docs/context` update
- [x] `decisions.md` update
