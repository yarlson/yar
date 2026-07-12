# YAR Decisions

This file records accepted, rejected, deferred, and withdrawn design decisions
so the same questions do not need to be re-litigated repeatedly.

Entries should be short and clear.

---

## Accepted

### Checking and code generation are separate stages

Status: accepted

Package loading, lowering, monomorphization, and semantic checking produce a
`CheckedProgram` that owns the program and its matching checker metadata. LLVM
generation accepts that paired value explicitly. The `check` command stops at
the checked-program boundary; commands that need IR or native code continue
through code generation.

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
Pointers, slices, maps, interfaces, functions, and file resource structs cannot
cross the boundary. Typed `net.Conn` and `net.Listener` values are deliberate
share-safe resource references. Results are exempt because the parent observes
them only after join.

Spawn targets are limited to named functions and immediately called inline
function literals so the checker can validate the complete boundary. Bare
`i64` handles remain indistinguishable from ordinary integer values. Direct
host-intrinsic spawns additionally require a task wrapper; currently only
`fs.read_file` provides one.

### Runtime handles are validated registry IDs

Status: accepted

User-visible file handles and compiler-internal network handles are positive
process-local opaque `i64` tokens rather than native addresses. Tokens identify
a kind-checked registry slot and generation and resolve to synchronized mutable
state. Vacant slots may be reused only after advancing the generation, which
changes the full token; stale generations remain invalid, and exhausted
generations retire their slots rather than wrapping.
`net.Conn` and `net.Listener` are typed, opaque, share-safe references; raw
network IDs are not public stdlib surface.

Network close linearizes when it removes the ID from the registry. It wakes
blocked accept, read, and write operations with `error.Closed`, then waits for
those operations and the host resource to finish releasing. A connection
permits one reader and one writer concurrently while serializing operations in
the same direction. File close remains non-interrupting and releases the host
file without an implicit durability sync.

Unknown, stale, and wrong-kind file or internal network IDs produce
`error.Closed`.
Invalid string-builder IDs terminate with the deterministic string-builder
runtime failure. This registry is a runtime safety boundary only: raw `i64`
values still have no compiler-visible nominal handle type or provenance.

### Sugar must lower to explicit semantics

Status: accepted

Language sugar is acceptable only when it maps cleanly onto simpler existing
semantics.

### Struct fields are package-private by default

Status: accepted

`pub field Type` explicitly exports a struct field. Same-package code may use
all fields, while external selector operations require public fields. If any
field is private, struct-literal construction belongs to the declaring package;
an all-private public struct is therefore opaque without an `opaque` keyword.
Private fields may use private types, public fields may not expose them, and
generic instantiations preserve declaration visibility and ownership. Enum
payload fields remain inherently public.

This boundary does not yet reject zero-value declarations or aggregate
zero-initialization of imported structs with private fields. Those construction
loopholes remain separate zero-value/initialization design work.

### Documentation authority is explicit

Status: accepted

`docs/YAR.md` is the public reference for implemented behavior. `docs/context/`
is the contributor-facing description of the current architecture and its
operational boundaries. This file records locked decisions. Proposals preserve
design history and may contain explicitly labeled deferred or superseded work;
they do not override the implemented reference or current architecture.

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
`pub`. A package's compiler identity is `PackageId = (source origin,
source-relative subpath)`; import text is only a binding. The compiler-owned
`std/<package>` namespace resolves exclusively to embedded stdlib before any
project or dependency lookup. Other imports are scoped to the importer origin:
same-origin packages, then aliases declared by that origin, then error.
Dependency aliases cannot be named `std`, and unresolved bare names of known
stdlib packages receive a migration diagnostic. Package graphs, lowering, and
cycle checks retain typed `PackageId` values, and lowering emits origin-safe
canonical symbols. Distinct imports with the same final qualifier segment are
rejected as ambiguous.

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
use the `_test.yar` suffix and are excluded from normal builds and imported
packages. `yar test` includes only the selected entry package's test files and
diagnoses every malformed `test_*` declaration. Valid tests follow the
`fn test_*(t *testing.T) void` convention without receivers, type parameters,
or an errorable return. Assertions use generic standalone functions
(`testing.equal[V]`) with rich failure messages via the `to_str` builtin. The
test runner is a generated Yar `main()` injected at compile time.

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
The prefix `yar --manifest-path <path/to/yar.toml> <command> ...` explicitly
selects one project and never falls back. Without it, compilation commands use
the nearest ancestor manifest from the entry directory, while dependency
commands use the nearest ancestor from the invocation directory. `init` always
targets the invocation directory or an explicit new root instead of discovering
ancestors. `add` creates in the invocation directory when discovery finds no
manifest or at an absent explicit target. Other explicit commands and
non-creating dependency commands require a manifest; automatic compilation may
remain manifestless. Invalid nearest candidates fail closed.
The selected manifest directory anchors `yar.lock`, transaction state, the root
package tree, and manifest-relative dependency paths while entry and output
arguments and program working directories remain invocation-relative.
The explicit `version = 1` lock graph records each git package's exact
commit/hash and full alias/git/ref child edges. Git declarations in the root
manifest and manifests of root path dependencies, plus every lock edge, must
match lock nodes exactly before dependency cache or network access. Duplicate
aliases or edges, missing nodes, cycles, source/ref conflicts, and unreachable
nodes are rejected. There is no root override.

Dependency bindings are owner-scoped. The entry origin receives root-manifest
aliases, each root path origin receives aliases from its own manifest, and each
locked git origin receives aliases from its lock node's child edges. A reachable
transitive alias is not visible unless the importing origin declares it. When a
locked dependency is selected, its cache tree is verified before its manifest
or source is read, then its declared git dependencies are checked against the
recorded child edges. Missing, mismatched, or edge-divergent selected trees fail
package loading; unused entries do not require a cache.

Lock v1 and the cache layout remain unchanged and retain global alias/source
uniqueness: the same alias cannot name different targets in different owner
scopes. True owner-local alias reuse requires lock v2. Source that relied on
global visibility of a reachable transitive alias must add a direct declaration
to the importing origin's manifest.

Fetching an existing lock verifies valid cached entries offline. Each missing
entry requests the locked commit object directly rather than re-resolving its
recorded ref; an unavailable object fails without ref fallback. Fresh checkouts
are verified before publication, and lock generation never derives a trusted
hash from cache content that differs from the fresh checkout.
Project dependency metadata is published as one recoverable transition.
`add`/`remove` compute and serialize the target manifest plus lock presence or
contents before publication; `lock`/`update` preserve manifest bytes. A
prepared journal restores the prior pair after pre-commit failure or
interruption, while a completion marker retains the target pair through
idempotent cleanup. Explicit selection recovers only its fixed project and never
falls back. Automatic discovery recognizes transaction state without a live
manifest, recovers that candidate, and restarts its ancestor search before
reading metadata. Success output follows commit and cleanup. Verified global
cache warming is outside this transaction. No other Yar CLI command may target
that selected project concurrently while metadata changes, even from another
invocation directory.
Local path dependencies remain live and unhashed and may be declared only in
the root manifest. Relative values are resolved from that manifest's directory.
Their manifests may contribute git roots but may not declare another path
dependency; locked git packages may not declare path dependencies either. A
selected path alias must exist and cannot fall through to a same-named stdlib
package. A targeted git update replaces its reachable graph, preserves nodes
unrelated to the selected graph, refreshes compatible shared nodes, and prunes
orphans; a targeted path update requires full `yar lock` reconciliation.

### Generics

Status: accepted

YAR supports generic structs and functions with explicit type parameters and
explicit type arguments at every use site. A monomorphization pass rewrites
instantiations before semantic checking and code generation. The current scope
has no inference, constraints, generic enums, generic methods, or methods on
instantiated generic types.

### Basic string operations

Status: accepted

YAR supports byte-oriented string operations: `len(str)`, `==`/`!=` comparison,
`+` concatenation, `s[i]` byte indexing returning `i32`, and `s[i:j]` byte
slicing returning `str`. Out-of-range operations trap at runtime.

### Standard library

Status: accepted

The compiler embeds a standard library written in Yar. Its packages use the
reserved import paths `std/strings`, `std/utf8`, `std/conv`, `std/sort`,
`std/path`, `std/fs`, `std/io`, `std/process`, `std/env`, `std/stdio`,
`std/net`, and `std/testing`. Direct and stdlib-internal imports
resolve only to the embedded stdlib origin and cannot be shadowed by project or
dependency sources.

### Text and UTF-8 helpers

Status: accepted

Helper functions for text-heavy programs live in stdlib packages (`strings`,
`utf8`, `conv`) rather than as builtins. Three new builtins (`chr`, `i32_to_i64`,
`i64_to_i32`) provide the minimal compiler-level support needed. The stdlib
packages provide `utf8.decode`, `utf8.width`, rune classification, integer
parsing, integer-to-string conversion, and single-byte string construction.

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

### Compiler CLI process contract

Status: accepted

Root and per-command help are side-effect-free information paths, and release
builds report the release version injected by the packaging system. `yar run`
accepts host program arguments only after an explicit `--` delimiter and
forwards them unchanged; the program's numeric exit status is propagated.

External native-build, test-binary, and Git work uses caller-created absolute
deadlines through one shared process-control boundary. Cargo and clang share a
build deadline, a generated test binary has its own deadline, and all Git
subprocesses in one dependency command share a deadline. A `yar run` user
program has no default deadline. Timed subprocesses use Unix process groups or
Windows Job Objects so cleanup follows descendant termination; Unix descendants
that deliberately create a new session remain outside process-group
containment. Missing executables are preserved as typed errors with the named
tool rather than flattened into generic I/O failures.

### Target runtime bundle contract

Status: accepted

Native builds consume one strict target runtime bundle rather than an untyped
archive path. The bundle declares an exact target triple, bundle-format epoch,
runtime-ABI epoch, compiler-compatibility epoch, one relative static archive,
and ordered native-library names. All metadata is validated before `clang`
runs; raw linker arguments are not bundle surface. `YAR_RUNTIME_BUNDLE` selects
an explicit bundle, release installations use `runtimes/<target-triple>/`, and
host source builds reuse the same checked-in manifests with a Cargo-built
archive. `YAR_RUNTIME_ARCHIVE` is rejected with migration guidance.

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

## Withdrawn

### The original HTTP server contract

Status: withdrawn

The original `std/http` experiment was removed because single-read request
parsing, ambiguous framing, unvalidated response headers, and an unbounded
connection lifecycle were not a safe server contract. HTTP serving requires a
new accepted design with bounded streaming, deadlines, strict framing, resource
ownership, and adversarial socket tests.

### Routing without a safe HTTP server substrate

Status: withdrawn

The routing proposal depended on the removed server contract. Its routing model
was not independently rejected; routing can be reconsidered after a safe HTTP
serving design reaches accepted status and its implementation is complete.

---

## Decision Update Rule

Any meaningful design decision should be recorded here when it becomes accepted,
rejected, explicitly deferred, or withdrawn.
