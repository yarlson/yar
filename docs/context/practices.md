# Practices

- The repository is organized as a single-project Go module with one
  user-facing runtime process: the `yar` CLI.
- Compilation is staged as package loading, package-graph lowering, semantic
  checking, LLVM IR generation, and optional native linking.
- Parse and semantic failures are returned as diagnostics; infrastructure
  failures such as file I/O or `clang` execution are returned as Go errors.
- Entry programs must declare `package main`.
- A user-defined `main` function is required, and it must return `i32` or
  `!i32`.
- Packages may span multiple `.yar` files in one directory.
- Imports are explicit `import "path"` declarations after the package clause.
- Imports resolve from local packages under the entry root first and fall back
  to the embedded stdlib only when the local package path is absent.
- Imported names stay package-qualified; imports do not inject unqualified
  exported names into local scope.
- Imported struct values may call exported methods through ordinary
  `value.method(...)` syntax.
- Top-level declarations may be `struct`, `interface`, `enum`, `fn`, or
  receiver-style method declarations, optionally prefixed with `pub`.
- Cross-package references may use only exported top-level declarations.
- Exported declarations may not expose package-local struct, interface, or enum
  types in their public surface.
- Import cycles are rejected.
- Package lowering rewrites package-local and imported declarations to
  canonical package-qualified names before checking and code generation.
- Explicit generic struct and function instantiations are monomorphized before
  semantic checking and code generation.
- Generic uses must supply explicit type arguments; the compiler does not infer
  them.
- The current generic system has no constraints.
- Local variables are introduced with `:=` or `var`, scoped by blocks, and may
  be reassigned only after declaration.
- Raw errorable values cannot be bound, assigned, passed as arguments, used in
  conditions, used in unary or binary operators, or accessed through fields or
  indexing.
- `&&` and `||` short-circuit in source order and require non-errorable `bool`
  operands.
- Errorable results must be handled immediately with direct `return`, `?`, or
  `or |err| { ... }`.
- `?` is front-end sugar for explicit error inspection and return from the
  current function.
- `or |err| { ... }` is front-end sugar for explicit local error inspection and
  handler control flow.
- Handler bindings introduced by `or |err| { ... }` are scoped to the handler
  block and have type `error`.
- The language supports both `!T` errorable returns and plain `error` values.
- The language supports user-defined structs, enums, fixed arrays, slices,
  maps, pointers, loops, and explicit assignment targets for locals, fields,
  indices, dereferences, and map elements.
- Methods are syntax over ordinary functions with an explicit receiver
  parameter.
- Function literals have explicit function types and lower to closure values
  carrying a code pointer plus an optional captured environment.
- Methods are allowed only on named local struct types, with either value
  receivers or pointer receivers.
- Methods cannot declare type parameters, and methods on instantiated generic
  types are not supported.
- Method calls require an exact receiver type match; the language does not add
  implicit `&` or `*` conversions.
- Method values are not first-class; `value.method` must be called immediately
  as `value.method(...)`.
- Closures capture outer locals lexically by value at closure creation time.
- Captured outer locals are readable inside closures but cannot be assigned
  through the closure body in the current implementation.
- Error names are collected across the program, sorted lexicographically, and
  then mapped to integer codes for the generated IR and native `main` wrapper.
- Builtins are compiler-owned contracts, not user-overridable functions,
  including collection helpers such as `len`, `append`, `has`, `delete`, and
  `keys`.
- Three builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to the
  standard library and rejected in user code by the package lowerer. User code
  accesses their functionality through the `conv` stdlib package.
- Map indexing returns an errorable value and uses `error.MissingKey` when the
  key is absent. `keys(map)` returns a snapshot slice of the current keys.
- The runtime C source is embedded in the Go binary and materialized into a
  temporary file during native builds.
- Runtime-managed allocation helpers back slices, maps, pointer-supporting
  storage, and other heap-backed features. The current runtime reclaims
  unreachable heap-backed objects with a conservative collector. Allocation
  failure is still an unrecoverable runtime failure rather than a YAR `error`.
- Files ending in `_test.yar` are excluded from `check`, `build`, `emit-ir`,
  and `run` commands. They are included only during `yar test`.
- Test functions follow the convention `fn test_*(t *testing.T) void` and are
  discovered at compile time by scanning test file ASTs.
- The `yar test` command generates a synthetic test runner `main()` that
  replaces the user `main()`, compiles the result, and executes it.
- `error.Name` expressions are valid both in return statements and as general
  expressions that produce values of type `error`.
- Error values support `==` and `!=` comparison.
- The `to_str` builtin is polymorphic and accepts `i32`, `i64`, `bool`, `str`,
  and `error` arguments. For error values, code generation emits a switch over
  the program-wide error-code table to produce `"error.Name"` strings.
- The CLI places a timeout around `build`, `run`, and `test` operations before
  invoking external processes.
- Cross-compilation is configured through `YAR_OS` and `YAR_ARCH` environment
  variables. The compiler maps the pair to an LLVM target triple internally.
  `yar run` rejects cross-compilation targets.
