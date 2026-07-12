# Compiler Pipeline

## Responsibility Split

- `crates/yar-cli` is the shipped CLI wiring. It parses command names and global
  project selection, resolves an entry file or package directory separately
  from its project root, compiles through `crates/yar-compiler`, formats
  diagnostics, resolves and validates runtime bundles, forwards delimited `run`
  arguments, and assigns operation deadlines to external build, test, and Git
  work.
- `crates/yar-process-control` is the shared subprocess boundary used by the CLI
  and dependency resolver. It owns typed start/timeout errors, concurrent output
  draining, deadline polling, timed descendant containment, cleanup, and Unix
  interrupt forwarding.
- `crates/yar-compiler/src/token.rs` defines the token type set, token values,
  and source positions used by the lexer, parser, and downstream stages.
- `crates/yar-compiler/src/diag.rs` defines the diagnostic type and accumulator
  used to collect source-positioned parse and semantic problems.
- `crates/yar-compiler/src/ast.rs` defines all AST node types plus the
  `Program`, `Package`, and `PackageGraph` containers used by single-file and
  package-graph compilation.
- `crates/yar-compiler/src/compile.rs` is the orchestration boundary. It
  exposes `check_path`, the full-pipeline `compile_path` and
  `compile_test_path` wrappers, entry-only test discovery, test declaration
  validation, and test runner generation.
- `crates/yar-compiler/src/lexer.rs` tokenizes source text, including control-flow, aggregate,
  pointer, and punctuation tokens, handles `//` comments and string escapes,
  and produces lexical diagnostics.
- `crates/yar-compiler/src/parser.rs` builds file ASTs, including top-level `struct`,
  `interface`, `enum`, `error`, `fn`, and receiver-style method declarations, optional
  `pub` export markers, explicit generic type parameter and type argument
  syntax, `import` declarations, function-literal and function-type syntax,
  loops, exhaustive enum `match` statements, array and slice literals, enum
  constructors, pointer types, `nil`, index and slice postfix forms,
  generalized lvalue forms such as `(*ptr).field`, method-call selector
  syntax, qualified call syntax, and sugar nodes for `?` and `or |err| { ... }`.
- `crates/yar-compiler/src/package.rs` resolves the package graph by explicit
  `PackageId` values made from a source origin and source-relative subpath. It
  preserves the entry directory beneath a separately selected project root,
  scopes lookup to the importer origin, verifies selected locked sources, seals
  stdlib imports, validates qualifiers and package names, and checks cycles.
- `crates/yar-compiler/src/manifest.rs` provides `yar.toml` and versioned
  `yar.lock` parsing, fetching, cache verification, and transitive resolution;
  callers supply one deadline shared across a dependency operation's Git
  subprocesses.
- `crates/yar-compiler/src/lock_graph.rs` reconciles manifest roots and full
  source/ref child edges before dependency cache access, rejects malformed or
  unreachable graphs, verifies selected cached manifests, and merges selective
  lock updates.
- `crates/yar-compiler/src/lower.rs` lowers the package graph into one combined
  `Program` by following resolved package identities and rewriting declarations
  to origin-safe canonical names. It preserves struct-field visibility, permits
  private fields to use private local types, and validates hidden-type exposure
  only through public fields and other exported signatures.
- `crates/yar-compiler/src/symbol.rs` owns those internal names and removes
  their reserved origin prefix from diagnostics before they leave the compiler.
- `crates/yar-compiler/src/mono.rs` monomorphizes explicit generic struct and
  function instantiations into ordinary declarations before checking.
- `crates/yar-compiler/src/checker.rs` validates struct, interface, enum, function, method, and
  function literal shape, tracks scopes, resolves builtin and rewritten user
  function signatures, resolves user-defined, enum, array, slice, map,
  pointer, function, and interface types, assigns expression types, records
  closure captures, validates exhaustive enum `match`, validates addressability
  and dereference rules, resolves concrete method lookup from receiver types,
  validates interface satisfaction and interface-method calls, validates loop
  and assignment-target rules, validates slice indexing/slicing and `append`,
  validates map key type restrictions, indexing, and `keys`, validates
  error-sugar legality, enforces package-owned private field selectors and
  struct-literal construction, preserves defining-package authority while
  checking function literals, validates declared error identities, and records
  their deterministic program-local code order.
- `crates/yar-compiler/src/codegen.rs` lowers the checked AST into LLVM IR, expanding concrete
  method calls into ordinary function calls with an explicit receiver argument,
  lowering interface values to boxed-data-plus-method-table pairs and
  interface-method calls to indirect dispatch through those tables, lowering
  function values to closure pairs of code pointer plus environment pointer,
  lowering function-literal captures into heap-backed environment objects,
  expanding error sugar, enum `match`, and short-circuit boolean operators into
  explicit checks, branches, and returns, lowering loops and aggregate values,
  lowering enums to tagged aggregates with aligned payload storage, lowering
  pointers to LLVM `ptr` values, lowering slices to runtime descriptors plus
  allocation/copy helpers, lowering maps to opaque runtime-managed hash tables
  with typed key/value access and key-snapshot extraction, generating the
  native `main` wrapper around `yar.main`, registering its outer stack boundary
  for conservative collection, and declaring the shared runtime allocation
  helpers used by heap-backed features.
- `stdlib/packages` contains the standard library written in Yar. The Rust
  package loader embeds those files behind the reserved `std/...` namespace.

## Stage Contracts

- `check_path` loads, lowers, monomorphizes, and checks a package graph. It
  returns a `CheckedProgram` only when those frontend stages have no
  diagnostics and never invokes LLVM generation.
- `CheckedProgram` owns a monomorphized `Program` and the matching
  `checker::Info`. Its private pairing is the input to
  `CheckedProgram::emit_llvm`; a generated `Unit` contains LLVM IR only.
- `compile_path` and `compile_test_path` are full-pipeline wrappers that compose
  the checked-program stage with explicit LLVM generation.
- Parse and semantic failures are returned as Yar diagnostics. Package-loading
  failures are `LoadError` values, and failures after the checked-program
  boundary are `CodegenError` values.
- Path compilation uses an explicitly supplied project root or discovers the
  nearest ancestor `yar.toml` from the entry directory. Without a manifest, the
  entry directory is the root. The entry must be within the selected root and
  receives a project-relative package subpath.
- The loader sorts file names inside each package directory, requires every
  file in a package directory to share the same package name, and rejects
  package directories without `.yar` files.
- Import paths are logical bindings interpreted within the importer origin.
  Absolute paths, dot-prefixed paths, empty segments, and invalid identifier
  segments are rejected.
- Imported package names must match the final segment of the import path, and
  `package main` cannot be imported.
- Distinct imports with the same final segment are rejected because that segment is the source qualifier.
- `std/<package>` resolves only to embedded stdlib before source or alias
  lookup. Other paths check packages in the importer origin, then aliases
  declared by that origin. Stdlib-origin imports also use `std/...`.
- Manifest roots and the versioned lock graph are reconciled before dependency
  cache access. Duplicate aliases or edges, missing nodes, source/ref
  mismatches, dependency cycles, and unreachable nodes stop package loading.
- A selected locked dependency is hash-verified before its manifest or source
  is parsed. The manifest's git dependencies must then match the recorded lock
  edges. Unused and locally shadowed lock entries are not opened. Compilation
  never repairs a missing or corrupt cache.
- A selected dependency must exist and receives no stdlib substitution.
  Unresolved bare stdlib names receive a migration diagnostic. Lock v1 retains
  global alias/source uniqueness even though alias visibility is owner-scoped.
- Code generation depends on `checker.Info` for expression types, function
  signatures, struct metadata, local types, and the program-wide canonical
  error-identity/code table.
- The checker and code generator operate on a monomorphized non-generic
  program; generic declarations do not reach those stages directly.
- Front-end sugar is preserved through parsing and semantic analysis, then
  lowered during code generation rather than being represented as a runtime
  feature.
- Methods follow the same pattern: receiver-aware syntax survives parsing and
  checking, then code generation emits ordinary function symbols and receiver
  arguments.
- Interfaces use a split model: declarations and satisfaction survive parsing
  and checking, then code generation emits boxed interface values plus
  per-interface method tables for indirect calls.
- Closures follow a similar split: function-literal syntax survives parsing and
  checking, then code generation emits synthetic functions plus explicit
  captured-environment objects.
- Generic instantiations follow a different pattern: generic syntax survives
  parsing and package lowering, then the compiler clones concrete declarations
  before semantic analysis while preserving every field's visibility.
- Heap allocation support is modeled as runtime helper calls and trap behavior
  rather than as part of the explicit source-level `error` system.
- The generated native `main` wrapper passes a stack-top pointer to the reserved
  runtime GC hook before user `yar.main()` executes. The current Rust runtime's
  hook is a no-op.
- Pointer-taking of locals and parameters is implemented conservatively by
  storing local slots in runtime-managed storage so returned or retained
  addresses stay valid without a separate escape-analysis pass.
- Native linking happens after IR generation by writing `main.ll`, validating a
  target runtime bundle, and invoking `clang` with its archive and ordered
  system-library metadata.
- When a cross-compilation target is specified via `YAR_OS`/`YAR_ARCH`, the
  generated IR includes a `target triple` directive and `clang` receives a
  `--target=<triple>` flag. Cross builds require a matching explicit or
  installed target runtime bundle.

## Generated Entry Boundary

- User code is emitted under `@yar.<function-name>`.
- The entry package keeps the user-defined `main` name; non-entry functions and
  imported declarations are rewritten to origin-safe canonical names before
  checking and code generation.
- Native process entry is a generated `@main` wrapper, not the user-defined
  function directly.
- Non-errorable `main` returns its `i32` result directly.
- Errorable `main` returns a generated result struct that the wrapper inspects
  to print an unhandled-error message or exit successfully.
