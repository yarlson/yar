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

- multi-file packages
- entry `package main` plus imports
- top-level `struct` and `fn`
- primitive types plus `error`
- fixed-size arrays
- slices
- typed pointers and recursive data
- enums with exhaustive `match`
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

- methods
- enums
- generics
- pattern matching
- exceptions

---

## v0.3 candidate capability: richer package ergonomics

Core code organization is now implemented. The next package-related work should
be smaller follow-on improvements rather than the initial package model.

### Candidate focus

- import aliases if they become necessary
- clearer package-root and module conventions
- richer diagnostics around package loading

---

## Accepted foundation: heap-backed feature memory model

The minimal runtime-managed memory model is now an accepted design foundation.

- future heap-backed features should reuse the shared allocation boundary
- allocation failure is outside the ordinary `error` model
- this is already used by slices and pointers, and remains groundwork for maps,
  richer string operations, and later heap-backed features

---

## v0.4 candidate capability: richer data modeling

Once organization is in place, richer domain modeling may become the next
priority.

### Candidate focus

- follow-on enum ergonomics once real pressure appears
- richer data-modeling features beyond the current enum and struct set
- maybe related pattern work if a small, coherent next step becomes clear

This area should be approached carefully because it has many interactions with:

- arrays
- structs
- errors
- future pattern matching ideas

---

## v0.5 candidate capability: self-hosting preparation

The language is now close to being able to express a self-hosted frontend in
memory. The next likely milestone is to close the remaining boundary gaps so
compiler and tooling programs become practical end to end.

### Candidate focus

- host filesystem and path access for package loading and artifact output
- host process, environment, stderr, and argv support for compiler CLI work
- map key enumeration so compiler maps can be traversed without maintaining
  duplicate side slices everywhere
- deterministic sorting helpers for stable diagnostics, package order, and error
  code assignment

### Candidate proposal set

- `0009-host-filesystem-and-path-utilities.md`
- `0010-host-process-and-environment.md`
- `0011-map-key-enumeration.md`
- `0012-sorting-helpers.md`

### v0.5 likely non-goals

- methods
- generics
- a full stream or descriptor API
- shell syntax or pipelines
- full map iterator protocols
- advanced module or package-manager design

---

## Backlog

These are interesting but not currently committed:

- methods
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
