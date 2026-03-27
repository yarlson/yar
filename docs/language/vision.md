# YAR Vision

## Purpose

YAR is a small compiled language with a Go-like feel, a stricter type discipline,
explicit error handling, and a bias toward simple semantics and fast compilation.

It should feel familiar to someone who likes Go’s readability and directness, but
it should permit a somewhat more refined surface where the added refinement pays
for itself in clarity and safety.

## Non-Goals

YAR is not trying to be:

- a kitchen-sink language
- a research language for type theory experiments
- a macro-heavy language
- an implicit or exception-driven language
- a language where cleverness is preferred over readability
- a language that grows by accumulating features without a strong interaction story

## Core Design Principles

### 1. Readable over clever

The language should be easy to scan and easy to explain.
Syntax should not require “decoding”.

### 2. Explicit over magical

Control flow should remain visible.
Errors, returns, and mutation should not hide behind surprising rules.

### 3. Small surface area

Every feature adds parser cost, checker cost, codegen cost, diagnostics cost,
test cost, and interaction cost.
The language should earn new surface area slowly.

### 4. Strict where it helps

The language may choose stricter rules than Go when those rules improve
correctness, reduce ambiguity, or simplify reasoning.

### 5. Sugar must be honest

Sugar is allowed when:

- it desugars cleanly into already-understood semantics
- it does not create a second semantic model
- diagnostics remain understandable
- control flow remains apparent

### 6. Errors are values

Errors are part of normal program logic.
The language does not use exception-style hidden unwinding as its primary model.

### 7. One coherent language

Features must fit together.
A feature that is individually appealing but awkward in combination with the rest
of the language should be rejected or deferred.

### 8. Implementation simplicity matters

Compiler simplicity is a language design constraint, not an afterthought.
A “nice” feature that causes disproportionate compiler complexity must justify
that cost clearly.

## Syntax Philosophy

YAR should generally prefer:

- short, direct forms
- keyword-light syntax where readability stays high
- familiar control-flow shapes
- a consistent, unsurprising grammar

YAR should generally avoid:

- symbolic soup
- multiple equivalent ways to express the same common operation
- syntax that is terse but unclear
- features whose edge cases dominate their value

## Type Philosophy

YAR should have a stronger, more explicit type discipline than Go where that
improves the language.

The type system should aim to be:

- easy to understand
- locally checkable
- explicit in important places
- strict enough to prevent sloppy usage patterns
- conservative about inference outside obvious cases

The language should resist type-system complexity that is hard to teach, hard to
diagnose, or hard to implement cleanly.

## Error Philosophy

Errors are explicit and handled in normal control flow.

The language should support:

- direct return of errors
- concise propagation sugar
- concise local handling sugar

The language should avoid:

- exception-style hidden control flow
- catch/throw style semantics as the default model
- sugar that obscures where an error goes

## Complexity Budget

For each milestone, prefer:

- at most one meaningful new data-modeling feature
- at most one meaningful new control-flow feature set
- no major feature without written semantics
- no major feature without interaction analysis
- no major feature without tests and current-state documentation updates

## Design Standard

A feature is a good fit for YAR when it is:

- easy to explain
- easy to read
- semantically boring in a good way
- implementable without heroic machinery
- consistent with the rest of the language
