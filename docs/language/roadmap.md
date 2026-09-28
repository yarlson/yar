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
- HTTP clients, TLS, keep-alive, or streaming bodies only through separate
  proposals grounded in the bounded server connection API;
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
