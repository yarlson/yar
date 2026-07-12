# Practices

- The repository is organized around a Rust 2024 `yar` CLI and Rust compiler
  crates.
- Package loading, graph lowering, monomorphization, and semantic checking
  produce a checked program. LLVM generation is an explicit downstream stage
  used only by commands that need IR or native code.
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
- An explicit `--manifest-path` selects the project root without fallback.
  Otherwise compilation discovers from the entry directory and dependency
  commands discover from the invocation directory; the nearest ancestor
  `yar.toml` wins. The entry remains separate from its selected project root.
- The root manifest's directory anchors its lock, transaction state, package
  tree, and relative dependency paths. Project selection does not change the
  process working directory or the base for source and output arguments.
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
- `yar.toml` and the presence or contents of `yar.lock` are one recoverable
  project-metadata state. Dependency mutations resolve and serialize the
  complete target state before publication; prepared transactions roll back
  the previous pair, while a completion marker retains the target pair through
  idempotent cleanup.
- `yar lock` and `yar update` preserve the manifest byte-for-byte. Removing all
  effective git roots publishes absence of `yar.lock`, not an empty lock file.
- Imported names stay package-qualified; imports do not inject unqualified
  exported names into local scope.
- Imported struct values may call exported methods through ordinary
  `value.method(...)` syntax.
- Top-level declarations may be `struct`, `interface`, `enum`, `error`, `fn`, or
  receiver-style method declarations, optionally prefixed with `pub`.
- Cross-package references may use only exported top-level declarations.
- Struct fields are package-private unless prefixed with `pub`. External
  selector operations require public fields, and any private field makes
  struct-literal construction package-owned. Same-package code retains full
  access; enum payload field visibility is unchanged.
- Exported declarations and public fields may not expose package-local struct,
  interface, or enum types. Private fields may use them.
- Generic instantiation preserves field visibility and declaration-package
  ownership. Function literals retain their defining package's private-field
  authority.
- Every source-level implicit zero is checked recursively at the use site.
  Initializer-free locals, omitted struct fields, and omitted fixed-array tails
  require an accessible zero value for the synthesized type.
- Scalars, strings, pointers, slices, interfaces, and channels have implicit
  zero values. Maps, functions, errors, and enums require explicit values;
  errorable values must be handled before ordinary binding.
- Struct zeroability follows field ownership and field types: a package may
  zero its own private fields, but cannot synthesize another package's private
  representation. Generic structs retain their declaration package as owner.
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
- Declared error identities are collected across the program, sorted by their
  origin-safe canonical package identity, and mapped to deterministic
  program-local integer codes for generated IR and the native `main` wrapper.
- Builtins are compiler-owned contracts, not user-overridable functions,
  including collection helpers such as `len`, `append`, `has`, `delete`, and
  `keys`.
- Three builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to the
  standard library and rejected in user code by the package lowerer. User code
  accesses their functionality through the `conv` stdlib package.
- Map indexing returns an errorable value and uses `error.MissingKey` when the
  key is absent. `keys(map)` returns a snapshot slice of the current keys.
- Native builds link `crates/yar-runtime` through a validated target bundle;
  bundle metadata owns the archive name and ordered native-library contract.
- Runtime-managed allocation helpers back slices, maps, pointer-supporting
  storage, and other heap-backed features. The Rust runtime reclaims unreachable
  blocks with a conservative non-moving collector. Allocation failure remains
  an unrecoverable runtime failure rather than a YAR `error`.
- String-builder and streaming-file handles plus compiler-internal network IDs
  are positive process-local opaque `i64` tokens, never native addresses. The
  registry validates slot generation and resource kind. Vacant slots may be
  reused only with a new generation and therefore a different full token; stale
  and wrong-kind attempts do not consume the live entry. Typed opaque `net.Conn`
  and `net.Listener` values are share-safe references; raw network IDs are
  internal.
- Network close linearizes at registry removal, wakes blocked accept/read/write
  operations with `error.Closed`, and waits for operation and resource release.
  Connections permit one concurrent reader and writer and serialize calls in
  the same direction. File close remains non-interrupting and performs no
  implicit durability sync. Unknown, stale, and wrong-kind IDs produce
  `error.Closed`; invalid string-builder IDs terminate with the string-builder
  runtime failure.
- Files ending in `_test.yar` are excluded from `check`, `build`, `emit-ir`,
  and `run` commands. `yar test` includes them only for its exact entry package;
  imported packages and dependencies remain production-only.
- Every entry test-file `test_*` declaration is validated during discovery.
  Tests require one `*testing.T` parameter, non-errorable `void`, no receiver,
  no type parameters, and the resolved `std/testing` import.
- The Rust token, lexer, and parser implementation owns lexical and syntactic
  acceptance; `docs/YAR.md` is its public current-language reference.
- `testdata/syntax_surface` is a portable accepted-syntax fixture. Language
  syntax changes keep it aligned with the frontend without encoding a
  consumer-specific syntax tree.
- External Tree-sitter and JetBrains repositories own their grammar sources,
  generated artifacts, editor queries, recovery behavior, tests, and releases.
- An external syntax projection declares the YAR revision it compares against,
  parses the portable fixture without parse-failure nodes, and validates its own
  negative, recovery, tree-shape, and query contracts before claiming parity.
- The `yar test` command generates a synthetic test runner `main()` that
  replaces the user `main()`, creates package-owned `testing.T` state through
  `testing.new`, reads results through methods, compiles the result, and executes
  it.
- Packages declare errors with `error Name` or `pub error Name`. Local
  `error.Name` and imported public `pkg.Name` expressions are valid both in
  return statements and as general values. Unknown spellings are rejected.
- Private errors can propagate through exported errorable APIs but cannot be
  named externally. Same-leaf errors from different package origins remain
  distinct.
- Error values support `==` and `!=` comparison.
- The `to_str` builtin is polymorphic and accepts `i32`, `i64`, `bool`, `str`,
  and `error` arguments. For error values, code generation emits a switch over
  the program-wide error-code table to produce legacy `"error.Name"` strings;
  this display is not an identity operation.
- Native build subprocesses share one absolute deadline configured by
  `YAR_BUILD_TIMEOUT_SECS` (30 seconds by default). Generated test binaries use
  `YAR_TEST_TIMEOUT_SECS` (30 seconds), while a `yar run` program has no default
  deadline. All Git subprocesses in one dependency command share
  `YAR_GIT_TIMEOUT_SECS` (300 seconds).
- Timed subprocesses use the shared `yar-process-control` boundary, which drains
  captured output, reports missing executables by name, forwards Unix
  interrupts, and terminates ordinary descendants through a Unix process group
  or Windows Job Object before temporary files are removed.
- Cross-compilation is configured through `YAR_OS` and `YAR_ARCH` environment
  variables. The compiler maps the pair to an LLVM target triple internally.
  `yar run` rejects cross-compilation targets.
