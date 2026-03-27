# YAR Design Process

This document defines how language design work is done.

It is about process, not language semantics.

## Goals

The process exists to ensure that language evolution is:

- deliberate
- coherent
- incremental
- documented
- testable
- resistant to feature drift

## Document Roles

### `vision.md`

Defines the stable identity and design principles of the language.

### `roadmap.md`

Defines planned milestones and candidate future directions.

### `current-state.md`

Describes only what the compiler implements today.

### `decisions.md`

Records important accepted, rejected, and deferred decisions.

### proposal docs

Describe specific features under consideration.

## Feature Lifecycle

A feature moves through these states:

1. idea
2. exploring
3. proposed
4. accepted
5. implemented
6. shipped

A feature may also end as:

- rejected
- deferred

## Meaning of Each State

### idea

A raw thought or possible direction.

### exploring

The problem space and alternatives are being examined.

### proposed

There is a written proposal with examples, semantics, and tradeoffs.

### accepted

The feature is approved for implementation.

### implemented

The compiler supports the feature.

### shipped

The feature is documented in `current-state.md` and considered part of the
language baseline.

### rejected

The feature is intentionally not part of the language.

### deferred

The feature may be worthwhile later, but is not appropriate now.

## Design Workflow

For each feature:

### 1. Capture the problem

Write down the concrete limitation or friction in the current language.

### 2. Explore alternatives

Consider at least two plausible designs, not just one.

### 3. Evaluate fit

Check whether the feature matches YAR’s principles.

### 4. Choose the smallest viable version

Prefer the smallest design that solves the actual problem.

### 5. Write a proposal

Use `proposal-template.md`.

### 6. Decide

Mark the proposal as accepted, rejected, or deferred.

### 7. Implement

Implementation should follow the accepted proposal, not replace it.

### 8. Test

Add parser, checker, lowering/codegen, and diagnostic tests as needed.

### 9. Update docs

After implementation, update:

- `current-state.md`
- `decisions.md`

## Required Questions for Every Proposal

A proposal is not complete until it answers:

- what problem does this solve?
- why is the existing language insufficient?
- what does the feature look like in valid code?
- what code is intentionally invalid?
- what are the exact semantics?
- how does it interact with current features?
- what does it cost?
- why is it worth doing now?

## Design Gates

Before accepting a proposal, check:

### Identity fit

Does it fit the language’s character?

### Real pressure

Does it solve a real current problem?

### Interaction clarity

Are interactions with existing features understood?

### Minimality

Is this the smallest version worth shipping?

### Milestone fit

Does it belong in the current planned milestone?

## Sugar Rule

A sugar feature is acceptable only if:

- it desugars cleanly
- the desugared form is already conceptually understood
- diagnostics remain understandable
- control flow remains visible
- it does not create a separate semantic world

## Capability-Based Planning

Milestones should be framed by newly enabled capabilities, not random isolated
features.

Good examples:

- complete basic boolean/control-flow expression writing
- organize programs across multiple files
- model richer domain data
- improve code reuse boundaries

Poor examples:

- add one operator
- add one keyword
- add one syntax form

A feature can be small, but the milestone should still answer:
“What new kind of program becomes practical?”

## Scope Discipline

Each milestone should stay intentionally small.

A milestone should not include multiple large interaction-heavy features unless
they are tightly connected and justified together.

## Implementation Rule

No meaningful feature should be implemented without:

- written semantics
- examples
- invalid examples
- interaction notes
- a decision state

## Documentation Rule

- `current-state.md` must describe reality only
- `roadmap.md` must not be treated as implemented truth
- proposal docs must not silently become canonical without an explicit decision

## Decision Rule

Any significant accepted, rejected, or deferred language choice should be
recorded in `decisions.md`.

## Process Review Rule

When the design process itself feels too heavy or too loose, update this file.
The process is allowed to evolve, but changes to process should also be
deliberate.
