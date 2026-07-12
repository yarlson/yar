# YAR Design Process

This document defines how language design work is proposed, decided, delivered,
and documented. It owns process and document roles, not language semantics.

## Truth and document ownership

Use the narrowest authoritative source for the question:

1. Compiler/runtime code and executable tests are the behavioral authority.
2. [`docs/YAR.md`](../YAR.md) is the canonical public reference for the
   currently implemented language, CLI, and standard library.
3. [`docs/context/`](../context/context-map.md) is the canonical current internal
   architecture, operational, and contributor context.
4. [`LLM.txt`](../../LLM.txt) is a compact derived mirror. It is never an
   independent authority.
5. [`decisions.md`](decisions.md) records concise design rationale so accepted,
   rejected, deferred, and withdrawn choices are not repeatedly re-litigated.
6. Proposal documents preserve design and implementation evidence for one
   bounded change. They do not override current code, tests, `docs/YAR.md`, or
   `docs/context/` when implementation later changes.
7. [`roadmap.md`](roadmap.md) contains future planning only.
8. [`vision.md`](vision.md) defines stable language identity and principles.

When two current-state documents disagree, verify behavior in code and tests,
then repair the owned public, internal, and derived surfaces together.

## Proposal registry

Each proposal's `Status` and `Implementation` fields are its canonical metadata.
[`README.md`](README.md) is the synchronized navigable registry of that metadata.
Update both in the same change; neither may disagree with the other.

The `design_records` integration guard runs through the existing
`cargo test --workspace` gate. It enforces proposal-header shape and allowed
values, one-to-one proposal/registry IDs, links, and metadata, decision-log
section/status coherence, preservation of the portable syntax fixture and its
compiler coverage registration, and absence of references to the removed
legacy current-state document. It checks record structure, not whether
proposal prose, implementation claims, checklists, roadmap priorities, or
external tooling projections are semantically correct.

## Two independent state axes

Design status and implementation delivery are separate. Never combine them in
one status phrase such as `accepted and implemented`.

### Design `Status`

Every proposal has exactly one design status:

- `exploring` — alternatives and constraints are still being investigated.
- `proposed` — a complete design is ready for a decision.
- `accepted` — the design is approved for implementation.
- `rejected` — the design was considered and intentionally declined.
- `deferred` — the design may be useful, but is not appropriate now.
- `withdrawn` — its author or maintainers retired the proposal before or after
  acceptance because its premise, substrate, or chosen design no longer holds.

Changing design status requires a short rationale in the proposal and, for a
meaningful language choice, a matching entry in `decisions.md`.

### Delivery `Implementation`

Every proposal records one independent delivery state:

- `not started` — no accepted implementation is present.
- `partial` — some accepted behavior exists, but the proposal is not complete.
- `implemented` — current code, tests, and current-state documentation cover the
  accepted design.
- `removed` — previously implemented behavior is no longer shipped.

`removed` is a delivery fact, not a design decision. A withdrawn proposal may
be `not started`, `partial`, `implemented`, or `removed` depending on history.
Code and executable tests settle disputed implementation state.

## Design workflow

### 1. Capture the problem

Describe the concrete limitation or friction in the current language.

### 2. Explore alternatives

Consider at least two plausible designs and record meaningful tradeoffs.

### 3. Evaluate fit

Check the design against `vision.md`, current semantics, and interaction costs.

### 4. Choose the smallest viable version

Prefer the smallest coherent design that solves the actual problem.

### 5. Write the proposal

Start from [`proposal-template.md`](proposal-template.md). Include valid and
invalid examples, exact semantics, interactions, alternatives, costs, and an
acceptance plan.

### 6. Decide

Set one design `Status` and record the decision rationale. Acceptance authorizes
implementation; it does not claim delivery.

### 7. Implement and test

Implementation follows the accepted contract. Add automated tests at the
highest-value boundary and keep the proposal checklist as evidence, not as the
current language reference.

### 8. Update owned current truth

When behavior changes:

- update `docs/YAR.md` for public language/API behavior;
- update only the affected `docs/context/` files for internal architecture or
  operations;
- update `LLM.txt` as the compact derived mirror;
- update `decisions.md` only when rationale or decision state changes;
- update `roadmap.md` only when future planning changes.

### 9. Record delivery

Update the proposal metadata and synchronized registry `Implementation` value
only after code, tests, and the owned current-state documentation agree. Use
`removed` when shipped behavior is deleted.

## Acceptance gates

Before accepting a proposal, verify:

- the problem is real and current;
- the design fits YAR's identity;
- semantics and invalid cases are explicit;
- interactions with existing features are understood;
- the design is the smallest coherent version;
- test and documentation obligations are concrete;
- unresolved questions do not change the public contract.

Before marking implementation complete, verify:

- code implements every accepted guarantee;
- automated tests cover behavior, edge cases, and failure paths;
- `docs/YAR.md` and affected `docs/context/` files describe reality;
- `LLM.txt` matches those current sources;
- the proposal registry and decision log are accurate.

## Scope discipline

Roadmap appearance is not acceptance. Proposal acceptance is not implementation.
Implementation is not current truth until code, tests, and owned documentation
agree.

No meaningful feature should be implemented without written semantics,
examples, invalid examples, interaction notes, a decision state, and an
acceptance plan.

When this process is too heavy or too loose, update this file deliberately.
