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
the current function.

### Local error handling with `or |err| { ... }`

Status: accepted

Local handling is supported as front-end sugar over explicit error checks and
control flow.

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
The dependency index is consulted between local and stdlib resolution during
package loading. Transitive dependencies are supported with conflict detection.

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
`sort`, `path`, `fs`, `process`, `env`, `stdio`.

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

YAR supports `+=`, `-=`, `*=`, `/=`, and `%=` as sugar that desugars to
`x = x op expr`. Works for integer arithmetic and string `+` concatenation.

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
