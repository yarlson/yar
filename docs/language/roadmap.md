# YAR Roadmap

This document contains future planning only. It is aspirational, not an
implemented-language reference and not a record of accepted decisions.

For current behavior, use [`docs/YAR.md`](../YAR.md). For accepted, rejected,
deferred, or withdrawn rationale, use [`decisions.md`](decisions.md) and the
[`proposal registry`](README.md).

## Planning rules

- Roadmap appearance does not imply proposal acceptance.
- Milestones are defined by newly enabled programming capabilities.
- Each milestone should remain intentionally small.
- Accepted scope requires a proposal and explicit decision.
- Implemented or removed work leaves this roadmap; history remains in proposals,
  decisions, and version control.

## Active proposed work

### Time values and UTC

Proposal [0024](proposals/0024-time.md) explores distinct timestamp, monotonic
instant, and duration values with a deliberately small UTC-first standard
library. It remains proposed and not started.

Before acceptance it must retain clear type separation, platform semantics,
overflow behavior, textual formats, and tests that do not depend on mutable
process-global timezone state.

## Future candidates

These are possible directions, not commitments:

- lock-format evolution if owner-local dependency alias reuse becomes necessary;
- import aliases if real programs expose qualifier ambiguity that package names
  cannot resolve cleanly;
- additional numeric types and explicit conversions when concrete interop or
  correctness requirements justify their surface area;
- pattern matching beyond the current exhaustive enum `match` only when a
  smaller data-modeling feature cannot solve the same programs;
- richer data modeling only when concrete programs expose a gap not solved by
  current structs, enums, interfaces, and generics;
- a new HTTP design only after bounded streaming, framing, deadlines, resource
  ownership, and adversarial socket behavior are specified together;
- additional standard-library capabilities driven by real compiler or tooling
  pressure;
- carefully scoped diagnostics and developer-experience improvements.

## Deferred by default

The following remain high-cost unless concrete pressure justifies a proposal:

- macros and large metaprogramming systems;
- operator overloading;
- exception-style hidden control flow;
- broad implicit conversions;
- syntax whose edge cases outweigh its capability gain;
- async or scheduler machinery without a workload that the native-thread model
  cannot serve safely.

## Promotion rule

A candidate becomes accepted work only after it has a proposal with examples,
semantics, invalid cases, interaction analysis, alternatives, complexity cost,
acceptance tests, and an explicit decision.
