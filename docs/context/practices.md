# Practices

- The repository is organized around a Rust 2024 `yar` CLI and Rust compiler
  crates.
- Compilation is staged as package loading, package-graph lowering, semantic
  checking, LLVM IR generation, and optional native linking.
- Parse and semantic failures are returned as diagnostics; infrastructure
  failures such as file I/O or `clang` execution are returned as host-language
  errors.
- Entry programs must declare `package main`.
- A user-defined `main` function is required, and it must return `i32` or
  `!i32`.
- Packages may span multiple `.yar` files in one directory.
- Imports are explicit `import "path"` declarations after the package clause.
- A loaded package is identified by `PackageId`: its source origin plus its
  source-relative package subpath. Import text is a binding request, not
  package identity.
- `std/<package>` is a compiler-owned namespace resolved exclusively to the
  embedded standard library before project or dependency lookup.
- Other imports resolve within the importing package's origin: the origin's
  own package tree first, then aliases declared for that origin, then error. An
  origin's self alias also resolves back into its own package tree.
- Entry-package aliases come from the root manifest, path-dependency aliases
  come from that dependency's manifest, and locked-package aliases come from
  that lock node's child edges. Reachability elsewhere in the graph does not
  make an alias visible.
- Dependency aliases cannot be named `std`. Embedded stdlib imports use the
  same canonical namespace and do not consult user-controlled sources.
- Bare user packages may share stdlib package names. An unresolved bare known
  stdlib name gets a migration diagnostic naming its `std/...` path.
- A selected dependency entry is authoritative; a missing declared path does
  not receive stdlib substitution.
- A versioned `yar.lock` records the complete reachable git dependency graph.
  Git declarations in the root manifest and manifests of root path
  dependencies, plus lock child edges, must agree on alias, git URL, ref kind,
  and ref value before cache or network access.
- Lock graphs reject duplicate aliases or edges, missing nodes, dependency
  cycles, and unreachable nodes. Lock v1 has global alias/source uniqueness:
  one alias cannot identify different source/ref tuples in different owner
  scopes. Representing that owner-local alias reuse requires a newer lock
  schema.
- When resolution selects a locked git dependency, its cache tree is verified
  against `yar.lock` before cached source is read. Its manifest is then checked
  against the node's recorded child edges. Unused or locally shadowed entries
  do not require a cache. Local path dependencies remain unhashed, may be
  declared only in the root manifest, and may not be nested through another
  path dependency or a locked git package.
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
  canonical origin-safe names before checking and code generation. Equal
  logical package paths from different origins cannot collide.
- The final import-path segment is the source qualifier. Two distinct imports
  with the same final segment are rejected instead of one silently replacing
  the other.
- Explicit generic struct and function instantiations are monomorphized before
  semantic checking and code generation.
- Generic uses must supply explicit type arguments; the compiler does not infer
  them.
- The current generic system has no constraints.
- Local variables are introduced with `:=` or `var`, scoped by blocks, and may
  be reassigned only after declaration.
- Raw errorable call expressions cannot be bound, assigned, passed as
  arguments, used in conditions, used in unary or binary operators, or
  accessed through fields or indexing.
- `&&` and `||` short-circuit in source order and require non-errorable `bool`
  operands.
- Errorable results must be handled immediately with direct `return`, `?`, or
  `or |err| { ... }`.
- First-class `!T` values produced by the language, such as taskgroup result
  elements, may be handled later with `?` or `or |err| { ... }`.
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
- `taskgroup` is an expression, `spawn` is a statement valid only inside a
  taskgroup body, and `spawn` is rejected inside function literals nested under
  a taskgroup body. `return` and same-function `?` propagation are rejected in
  taskgroup bodies so every accepted path reaches the join.
- Spawn targets are named functions or immediately called inline literals.
  Their arguments and captures must be recursively share-safe; task results
  are observed only after the group joins and have no such restriction. Direct
  host intrinsics additionally need task-wrapper support; currently only
  `fs.read_file` has it.
- `chan[T]` is a builtin type. Channel element types cannot be `void`,
  `noreturn`, or another channel type.
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
- Native builds link the Rust runtime archive from `crates/yar-runtime`.
- Runtime-managed allocation helpers back slices, maps, pointer-supporting
  storage, and other heap-backed features. The current Rust runtime retains
  allocations until process exit. Allocation failure is still an unrecoverable
  runtime failure rather than a YAR `error`.
- User-visible `i64` handles for string builders, streaming files, TCP
  listeners, and TCP connections are process-local registry IDs, never native
  addresses. The registry validates both ID and resource kind before access,
  never reuses an issued ID, and synchronizes mutable per-handle state.
- Explicit file and network close first removes the ID so new lookups fail,
  then waits for any operation holding the per-resource lock before releasing
  the host resource. Close does not interrupt blocking I/O. Unknown, stale, and
  wrong-kind file or network IDs produce `error.Closed`; invalid string-builder
  IDs terminate with the string-builder runtime failure.
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
