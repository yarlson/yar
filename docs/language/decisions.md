# YAR Decisions

This file records accepted, rejected, and deferred design decisions so the same
questions do not need to be re-litigated repeatedly.

Entries should be short and clear.

---

## Accepted

### Errors are values

Status: accepted

YAR uses explicit error values and error-aware return types as its primary error
model.

### Propagation sugar with `?`

Status: accepted

`expr?` is allowed on `!T` and `error` expressions and propagates failure from
the current function. It is rejected inside a taskgroup body because every
accepted taskgroup path must reach the join; nested function literals are
separate propagation scopes.

### Local error handling with `or |err| { ... }`

Status: accepted

Local handling is supported as front-end sugar over explicit error checks and
control flow.

### Spawn boundaries require share-safe values

Status: accepted

Each `spawn` starts a native thread with shallow copies of its arguments and
inline-literal captures. The checker therefore permits only transitively
share-safe inputs: scalars, strings, errors, errorable values and channels with
share-safe payloads, and aggregates composed entirely from share-safe values.
Pointers, slices, maps, interfaces, functions, and resource structs cannot
cross the boundary. Results are exempt because the parent observes them only
after join.

Spawn targets are limited to named functions and immediately called inline
function literals so the checker can validate the complete boundary. Bare
`i64` handles remain indistinguishable from ordinary integer values. Direct
host-intrinsic spawns additionally require a task wrapper; currently only
`fs.read_file` provides one.

### Runtime handles are validated registry IDs

Status: accepted

User-visible `i64` handles for string builders, streaming files, TCP listeners,
and TCP connections are positive process-local registry IDs rather than native
addresses. IDs are kind-checked, never reused within a process, and resolve to
synchronized mutable state. Explicit file and network close removes the ID so
new lookups fail, then waits for any operation holding the per-resource lock
before releasing the host resource; close does not interrupt blocking I/O.

Unknown, stale, and wrong-kind file or network IDs produce `error.Closed`.
Invalid string-builder IDs terminate with the deterministic string-builder
runtime failure. This registry is a runtime safety boundary only: raw `i64`
values still have no compiler-visible nominal handle type or provenance.

### Sugar must lower to explicit semantics

Status: accepted

Language sugar is acceptable only when it maps cleanly onto simpler existing
semantics.

### Current-state docs are descriptive only

Status: accepted

`current-state.md` documents what the compiler actually implements, not future
plans.

### Heap memory is runtime-managed

Status: accepted

Heap-backed features must use one runtime-managed memory model: user code does
not manually free storage, and allocation failure is an unrecoverable runtime
failure rather than part of the ordinary `error` model.

### Garbage collection stays runtime-only

Status: accepted

YAR may reclaim unreachable heap-backed storage, but collection remains a
runtime concern rather than a user-visible language feature. There is no
`gc()` builtin, no manual `free`, and no source-level promise about exact
collection timing.

### Boolean operators are short-circuiting

Status: accepted

`&&` and `||` are supported for `bool` operands and lower to explicit
short-circuit control flow rather than eager evaluation.

### Integer arithmetic is wrapping with explicit division traps

Status: accepted

For `i32` and `i64`, addition, subtraction, multiplication, and unary negation
wrap to the operand width. Division and remainder terminate deterministically
for a zero divisor and for the signed overflow pair `MIN` and `-1`.

### Bare `for` loops

Status: accepted

`for { ... }` is an unconditional loop form equivalent to a true condition.
It reuses existing `break` and `continue` semantics and does not introduce a
separate `while` keyword.

### Imports and multi-file packages

Status: accepted

Packages may span multiple files, imports are explicit, cross-package references
stay qualified, and top-level declarations are package-local unless marked
`pub`.

### Slices

Status: accepted

YAR supports `[]T`, slice literals, `s[i:j]`, indexing, `len(slice)`, and
explicit `append(slice, value)` reassignment with runtime bounds checks.

### Typed pointers and recursive data

Status: accepted

YAR supports explicit `*T` pointers, `&expr`, `*expr`, `nil`, and recursive
data through pointer indirection rather than direct inline containment.
Dereferencing `nil` terminates with a deterministic runtime error.

### Enums and exhaustive `match`

Status: accepted

YAR supports closed enums with plain and payload cases plus statement-form,
exhaustive `match` over enum values.

### Host filesystem and path stdlib

Status: accepted

YAR exposes host filesystem access through stdlib packages rather than new
syntax-level builtins. `path` stays pure Yar code, while the small `fs` surface
lowers to runtime shims with stable user-visible error names.

### Sorting helpers

Status: accepted

YAR exposes deterministic in-place sorting through a small stdlib `sort`
package rather than new builtins or syntax. The initial surface is
`sort.strings([]str)`, `sort.i32s([]i32)`, and `sort.i64s([]i64)`.

### Methods

Status: accepted

YAR supports methods on named struct types with explicit value or pointer
receivers. Method calls use `value.method(...)`, exported methods use `pub`,
receiver matching is exact, and methods lower to ordinary functions with an
explicit receiver argument.

### Closures

Status: accepted

YAR supports anonymous function literals and first-class function types.
Closures capture outer locals lexically by value, calls through function
values are explicit, and captured outer bindings are read-only inside closure
bodies in the current implementation.

### Interfaces

Status: accepted

YAR supports named interfaces with method requirements only. Concrete
satisfaction is implicit, exact receiver matching still applies, interface
values lower to boxed data plus method tables, and interface-to-interface
conversion is limited to the same exact interface type in the current
implementation.

### Testing framework with `yar test`

Status: accepted

YAR provides a `testing` stdlib package and a `yar test` CLI command. Test files
use the `_test.yar` suffix and are excluded from normal builds. Test functions
follow the `fn test_*(t *testing.T) void` convention and are discovered at
compile time. Assertions use generic standalone functions (`testing.equal[V]`)
with rich failure messages via the `to_str` builtin. The test runner is a
generated Yar `main()` injected at compile time.

### `to_str` builtin

Status: accepted

YAR provides a compiler-provided `to_str` builtin that converts primitive values
(`i32`, `i64`, `bool`, `str`, `error`) to their string representation. This is
polymorphic like `len` — the compiler selects the conversion strategy based on
argument type. This eliminated the need for type-specific assertion functions in
the testing package.

### Error comparison and error expressions

Status: accepted

Error values support `==` and `!=` comparison. `error.Name` expressions are
valid outside return statements, enabling patterns like
`testing.equal[error](t, err, error.NotFound)`. Errors are `i32` codes
internally, so comparison uses integer `icmp`.

### Dependency management

Status: accepted

Yar supports external dependencies through `yar.toml` manifests and `yar.lock`
lock files. Dependencies are git repositories identified by short alias names.
There is no central registry, no semver range resolution, and no parser changes.
The explicit `version = 1` lock graph records each git package's exact
commit/hash and full alias/git/ref child edges. Git declarations in the root
manifest and manifests of root path dependencies, plus every lock edge, must
match lock nodes exactly before dependency cache or network access. Duplicate
aliases or edges, missing nodes, cycles, source/ref conflicts, and unreachable
nodes are rejected. There is no root override.

The dependency index is global: every alias reachable in the reconciled graph
may be imported by any loaded package, including a transitive alias not
declared directly by the importer. It is consulted between local and stdlib
resolution. When a locked dependency is selected, its cache tree is verified
before its manifest or source is read, then its declared git dependencies are
checked against the recorded child edges. Missing, mismatched, or
edge-divergent selected trees fail package loading; unused or locally shadowed
entries do not require a cache.

Fresh fetches are verified before publication, and lock generation never
derives a trusted hash from cache content that differs from the fresh checkout.
Local path dependencies remain live and unhashed and may be declared only in
the root manifest. Their manifests may contribute git roots but may not declare
another path dependency; locked git packages may not declare path dependencies
either. A selected path alias must exist and cannot fall through to a
same-named stdlib package. A targeted git update replaces its reachable graph,
preserves nodes unrelated to the selected graph, refreshes compatible shared
nodes, and prunes orphans; a targeted path update requires full `yar lock`
reconciliation.

---

## Rejected

### `try` / `catch` style default error model

Status: rejected

YAR does not use exception-style primary error handling.

### Hidden exception-like control flow

Status: rejected

The language should not hide non-local control flow behind implicit mechanisms.

### Feature growth without written semantics

Status: rejected

Features should not be added purely from intuition or implementation momentum.

---

## Deferred

### Generics

Status: deferred

Too large and too interaction-heavy for the current stage of the language.

### Basic string operations

Status: accepted

YAR supports byte-oriented string operations: `len(str)`, `==`/`!=` comparison,
`+` concatenation, `s[i]` byte indexing returning `i32`, and `s[i:j]` byte
slicing returning `str`. Out-of-range operations trap at runtime.

### Standard library

Status: accepted

The compiler embeds a standard library written in Yar. Stdlib packages are
resolved as a fallback when an import path does not match a local directory.
Local packages take priority over stdlib. Packages: `strings`, `utf8`, `conv`,
`sort`, `path`, `fs`, `io`, `process`, `env`, `stdio`, `net`, `http`, and
`testing`.

### Text and UTF-8 helpers

Status: accepted

Helper functions for text-heavy programs live in stdlib packages (`strings`,
`utf8`, `conv`) rather than as builtins. Three new builtins (`chr`, `i32_to_i64`,
`i64_to_i32`) provide the minimal compiler-level support needed. The stdlib
packages provide `utf8.decode`, `utf8.width`, rune classification, integer
parsing, integer-to-string conversion, and single-byte string construction.

---

### Maps

Status: accepted

Maps are a built-in `map[K]V` associative container. Key types are restricted to
`bool`, `i32`, `i64`, and `str`. Map lookup returns `!V` with `error.MissingKey`
on absent keys, keeping missing-key handling explicit and compatible with YAR's
error model. `has`, `delete`, `keys`, and `len` are provided as builtins.
`keys(m)` returns a snapshot `[]K` with unspecified order. Maps are
heap-allocated opaque handles backed by an open-addressing hash table in the
runtime. The current surface has no live iteration protocol, ordering
guarantees, or set syntax.

### Compound assignment operators

Status: accepted

YAR supports `+=`, `-=`, `*=`, `/=`, and `%=` for integer arithmetic and string
`+` concatenation. The assignment target is evaluated exactly once. Map
elements do not support compound assignment because lookup is errorable.

### Open-ended slice syntax

Status: accepted

YAR supports `s[i:]` (end defaults to `len(s)`) and `s[:j]` (start defaults to
`0`) for both slices and strings. The full `s[i:j]` form remains supported.

### Single-field enum positional constructors

Status: accepted

Payload enum cases with exactly one field accept positional syntax:
`Enum.Case(value)` as sugar for `Enum.Case{field: value}`. Multi-field cases
keep keyed syntax only. The keyed form remains valid for all cases.

---

## Decision Update Rule

Any meaningful design decision should be recorded here when it becomes accepted,
rejected, or explicitly deferred.
