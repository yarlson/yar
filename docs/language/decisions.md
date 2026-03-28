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

### Methods

Status: deferred

Deferred until there is a stronger need for associated behavior and a clear
interaction story with structs and future modules.

### Generics

Status: deferred

Too large and too interaction-heavy for the current stage of the language.

---

### Maps

Status: accepted

Maps are a built-in `map[K]V` associative container. Key types are restricted to
`bool`, `i32`, `i64`, and `str`. Map lookup returns `!V` with `error.MissingKey`
on absent keys, keeping missing-key handling explicit and compatible with YAR's
error model. `has`, `delete`, and `len` are provided as builtins. Maps are
heap-allocated opaque handles backed by an open-addressing hash table in the
runtime. The first version has no iteration, ordering guarantees, or set syntax.

---

## Decision Update Rule

Any meaningful design decision should be recorded here when it becomes accepted,
rejected, or explicitly deferred.
