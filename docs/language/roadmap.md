# YAR Roadmap

This document tracks intended language evolution.

Unlike `current-state.md`, this document is aspirational. It describes what the
language is expected to grow toward, not necessarily what the compiler already
implements today.

## Planning Rules

- milestones are defined by newly enabled programming capabilities
- each milestone should stay small
- accepted scope should be explicit
- deferred items should be recorded clearly
- current-state documentation remains separate and descriptive only

---

## v0.1 / current baseline

Current baseline already includes:

- one-file programs
- `package main`
- top-level `struct` and `fn`
- primitive types plus `error`
- fixed-size arrays
- `if`, `else`, `for`, `break`, `continue`
- boolean operators `&&` and `||`
- explicit error model with `!T`, `?`, and `or |err| { ... }`
- LLVM-based native code generation

See `current-state.md` for exact truth.

---

## v0.2 goal

v0.2 should make the language more complete and internally coherent, while
avoiding a large jump in complexity.

### Candidate focus areas

#### 1. Broader expression completeness

Consider whether additional expression polish is needed, but keep scope tight.

#### 2. Language consistency pass

Tighten rough edges in:

- type rules
- error restrictions
- literal behavior
- diagnostics
- statement/expression boundaries

The goal is not lots of new features, but a cleaner and more uniform language.

### v0.2 likely non-goals

- imports
- multi-file packages
- methods
- enums
- slices
- generics
- pattern matching
- exceptions

---

## v0.3 candidate capability: code organization

The next major missing capability after baseline completeness is code
organization beyond a single file.

### Candidate focus

- imports
- multi-file packages
- symbol visibility / package boundaries as needed

This milestone should aim to make larger programs possible.

### Explicit caution

Do not start this milestone until:

- name resolution rules are written down
- package/file ownership semantics are clear
- builtin and package symbol interactions are specified

---

## Accepted foundation: heap-backed feature memory model

The minimal runtime-managed memory model is now an accepted design foundation.

- future heap-backed features should reuse the shared allocation boundary
- allocation failure is outside the ordinary `error` model
- this is groundwork, not a claim that pointers, slices, maps, or string
  concatenation are already shipped

---

## v0.4 candidate capability: richer data modeling

Once organization is in place, richer domain modeling may become the next
priority.

### Candidate focus

- enums or tagged unions
- improved sum-like modeling
- maybe related exhaustiveness checks if the model supports them

This area should be approached carefully because it has many interactions with:

- arrays
- structs
- errors
- future pattern matching ideas

---

## Backlog

These are interesting but not currently committed:

- slices
- methods
- imports
- multi-file packages
- enums / tagged unions
- richer builtin library
- more numeric types
- explicit conversion syntax
- pattern matching
- interfaces / traits
- generics
- concurrency primitives

---

## Deferred by default

These items should be considered high-cost and deferred unless a strong case is
made:

- macros
- operator overloading
- exceptions
- implicit conversions
- hidden control-flow features
- large metaprogramming systems

---

## Roadmap Discipline

A roadmap item is not accepted just because it appears here.

For a feature to become accepted work, it should have:

- a proposal doc
- examples
- semantics
- interaction analysis
- a clear milestone fit
