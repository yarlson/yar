# Compiler Pipeline

## Responsibility Split

- `crates/yar-cli` is the shipped CLI wiring. It parses command names and basic
  arguments, compiles an entry file or package directory through
  `crates/yar-compiler`, formats diagnostics, resolves runtime archives, and
  sets a timeout for `build`, `run`, and `test`.
- `crates/yar-compiler/src/token.rs` defines the token type set, token values,
  and source positions used by the lexer, parser, and downstream stages.
- `crates/yar-compiler/src/diag.rs` defines the diagnostic type and accumulator
  used to collect source-positioned parse and semantic problems.
- `crates/yar-compiler/src/ast.rs` defines all AST node types plus the
  `Program`, `Package`, and `PackageGraph` containers used by single-file and
  package-graph compilation.
- `crates/yar-compiler/src/compile.rs` is the orchestration boundary. It
  exposes `compile_source`, `compile_path`, `compile_test_path`, test discovery,
  and test runner generation.
- `crates/yar-compiler/src/lexer.rs` tokenizes source text, including control-flow, aggregate,
  pointer, and punctuation tokens, handles `//` comments and string escapes,
  and produces lexical diagnostics.
- `crates/yar-compiler/src/parser.rs` builds file ASTs, including top-level `struct`,
  `interface`, `enum`, `fn`, and receiver-style method declarations, optional
  `pub` export markers, explicit generic type parameter and type argument
  syntax, `import` declarations, function-literal and function-type syntax,
  loops, exhaustive enum `match` statements, array and slice literals, enum
  constructors, pointer types, `nil`, index and slice postfix forms,
  generalized lvalue forms such as `(*ptr).field`, method-call selector
  syntax, qualified call syntax, and sugar nodes for `?` and `or |err| { ... }`.
- `crates/yar-compiler/src/package.rs` resolves the package graph. It loads local
  `.yar` files from disk, consults the dependency index built from `yar.toml`
  and `yar.lock` metadata for external packages, verifies a locked cache when
  its alias is selected, falls back to embedded stdlib packages when both local
  and dependency paths are missing, validates package names, and checks import
  cycles.
- `crates/yar-compiler/src/manifest.rs` provides dependency management infrastructure including
  `yar.toml` and `yar.lock` parsing, git-based fetching, lock-hash verification
  for commit-keyed cache entries, transitive resolution with
  conflict detection, and the alias-to-path index consumed by the package
  loader.
- `crates/yar-compiler/src/lower.rs` lowers the package graph into one combined
  `Program` by rewriting package-local and imported symbols to canonical names.
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
  error-sugar legality, and records ordered error names.
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
  native `main` wrapper around `yar.main`, calling the runtime's reserved GC
  initialization hook there, and declaring the shared runtime allocation
  helpers used by heap-backed features. The current Rust runtime leaves that GC
  hook inactive and retains allocations until process exit.
- `stdlib/packages` contains the standard library written in Yar. The Rust
  package loader embeds those files and uses them as the final fallback after
  local and dependency imports.

## Stage Contracts

- `Compile` returns a `Unit` only when parse and semantic checking succeed.
- Diagnostics stop code generation but are reported as Yar diagnostics rather
  than host-language errors.
- `Compile` works on one already-loaded source string. `CompilePath` is the
  path-based entrypoint that supports packages, imports, stdlib fallback, and
  export validation.
- The loader sorts file names inside each package directory, requires every
  file in a package directory to share the same package name, and rejects
  package directories without `.yar` files.
- Import paths are logical package paths rooted at the entry directory.
  Absolute paths, dot-prefixed paths, empty segments, and invalid identifier
  segments are rejected.
- Imported package names must match the final segment of the import path, and
  `package main` cannot be imported.
- Local packages shadow embedded stdlib packages with the same import path.
- A selected locked dependency is verified before cached source parsing or
  stdlib fallback. Unused and locally shadowed lock entries are not verified.
  Compilation never repairs a missing or corrupt cache.
- Code generation depends on `checker.Info` for expression types, function
  signatures, struct metadata, local types, and the program-wide error-code
  table.
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
  before semantic analysis.
- Heap allocation support is modeled as runtime helper calls and trap behavior
  rather than as part of the explicit source-level `error` system.
- The generated native `main` wrapper passes a stack-top pointer to the reserved
  runtime GC hook before user `yar.main()` executes. The current Rust runtime's
  hook is a no-op.
- Pointer-taking of locals and parameters is implemented conservatively by
  storing local slots in runtime-managed storage so returned or retained
  addresses stay valid without a separate escape-analysis pass.
- Native linking happens after IR generation by writing `main.ll`, selecting a
  Rust runtime archive, and invoking `clang`.
- When a cross-compilation target is specified via `YAR_OS`/`YAR_ARCH`, the
  generated IR includes a `target triple` directive and `clang` receives a
  `--target=<triple>` flag. Cross builds require `YAR_RUNTIME_ARCHIVE`.

## Generated Entry Boundary

- User code is emitted under `@yar.<function-name>`.
- The entry package keeps the user-defined `main` name; non-entry functions and
  imported declarations are rewritten to canonical package-qualified names
  before checking and code generation.
- Native process entry is a generated `@main` wrapper, not the user-defined
  function directly.
- Non-errorable `main` returns its `i32` result directly.
- Errorable `main` returns a generated result struct that the wrapper inspects
  to print an unhandled-error message or exit successfully.
