# AGENTS.md

## Implementation Process

For implementation work in this repository, follow this order:

1. read `docs/context/`
2. plan implementation and the test approach before changing code
3. write or update tests first when practical; prefer TDD for new behavior, bug fixes, and regressions
4. implement
5. update `testdata/` meaningfully and reasonably when behavior, fixtures, or representative programs change
6. run `golangci-lint run --fix ./...` and `go test -race -count=1 -v -timeout=120s ./...`
7. fix issues from lint and tests
8. update `docs/context/`
9. update `docs/YAR.md`
10. update `docs/language`
11. use `/review` slash command from `~/.claude/commands/review.md` or `review` skill to review the code
12. fix all found issues
13. run `golangci-lint run --fix ./...` and `go test -race -count=1 -v -timeout=120s ./...`
14. fix issues from lint and tests
15. if needed, update `docs/context/`, `docs/YAR.md`, `docs/language`

Do not skip the documentation updates when implementation changes behavior,
capabilities, constraints, runtime details, or accepted language design.

Do not skip test work for implementation changes. Add or update automated tests at
the highest-value layer for the change, and keep `testdata/` focused on
representative, durable fixtures rather than incidental cases.

## Go Code Quality Standard

Write Go for readability, correctness, maintainability, security, and
performance.

Priority order:

1. readability
2. correctness
3. maintainability
4. performance

Prefer explicit, idiomatic, production-grade code. Do not trade correctness or
clarity for speculative optimization.

---

## Core Principles

- Prefer simple, idiomatic Go over cleverness.
- Keep control flow, ownership, and mutation explicit.
- Prefer composition and direct wiring over heavy abstraction.
- Avoid framework-like patterns, unnecessary indirection, and speculative design.

---

## Package Design

- Keep packages small, focused, and acyclic.
- Keep public APIs minimal and hide implementation details by default.
- Prefer shallow package structure and clear boundaries.
- Do not add layers unless they materially improve clarity.

---

## Interfaces

- Accept interfaces, return structs.
- Use concrete types by default.
- Define small interfaces near the consumer.
- Do not add interfaces just for future flexibility or mocking.

---

## Types and API Design

- Make invalid states hard to represent.
- Be explicit about ownership, lifecycle, concurrency, and zero-value behavior.
- Keep APIs small, direct, and hard to misuse.
- Use constructors and abstraction only when they add clear value.

---

## Naming

- Use clear, domain-specific, descriptive names.
- Prefer intent-revealing names over vague buckets like `util`, `helper`, or `manager`.
- Keep package names short and idiomatic.

---

## Functions and Methods

- Keep functions small, cohesive, and easy to scan.
- Prefer straightforward control flow and early returns.
- Be explicit about mutation, ownership, and receiver choice.
- Do not extract helpers that make the call site harder to understand.

---

## Error Handling

- Handle errors explicitly and never ignore them without a clear reason.
- Wrap with `%w` when extra context helps.
- Do not use `panic` for normal error handling.
- Validate inputs and make edge cases explicit at boundaries.

---

## Context Usage

- Pass `context.Context` explicitly when appropriate, usually as the first parameter.
- Never store context in structs.
- Propagate caller context through request boundaries.
- Respect cancellation, deadlines, and timeouts.

---

## Concurrency

- Do not add concurrency unless it is needed and beneficial.
- Prefer simple synchronization and data flow.
- Be explicit about concurrent-safety guarantees.
- Avoid leaks, races, deadlocks, and hidden shared mutable state.

---

## Performance

- Measure before optimizing.
- Prefer simple algorithms and data structures.
- Avoid unnecessary allocations, copies, conversions, reflection, and boxing.
- Do not trade maintainability for hypothetical speedups.

---

## Security

- Treat all external input as untrusted.
- Validate, sanitize, and bound data at system boundaries.
- Use secure defaults and least privilege.
- Never log secrets or introduce avoidable data-exposure risks.

---

## State and Configuration

- Minimize global state and hidden runtime coupling.
- Prefer explicit dependencies and explicit configuration.
- Keep initialization obvious and remove speculative extension points.

---

## Comments and Documentation

- Write comments only when they add signal.
- Explain why or document non-obvious invariants and tradeoffs.
- Keep comments and exported docs accurate.

---

## Testing

- Test behavior, edge cases, failure paths, and regressions.
- Prefer deterministic tests and control time, randomness, filesystem, process, and concurrency effects.
- Choose the highest-value test layer first; test user-visible flows at the boundary that best exercises them.
- Add lower-level tests to protect pure logic and isolate failures.
- Avoid brittle tests and duplicated assertions across layers unless each layer catches different risks.

---

## Dependency Management

- Prefer the standard library first.
- Add third-party dependencies only when clearly justified.
- Avoid dependencies that add more abstraction than value.

---

## Change Guidelines

- Preserve good existing style.
- Prefer minimal, high-signal changes over broad rewrites.
- Improve naming, structure, and boundaries where needed.
- Call out tradeoffs for security-sensitive, performance-sensitive, or otherwise non-obvious changes.

---

## Review Checklist

Before finalizing a Go change, verify readability, correctness on edge cases,
explicit error handling, justified abstractions, safe boundary validation,
necessary and safe concurrency, reasonable performance, explicit security
handling, minimal global state, and maintainability.

---

## Hard Rules

- Prefer simple, explicit code.
- Accept interfaces, return structs.
- Handle errors explicitly.
- Pass context explicitly.
- Avoid global state.
- Avoid premature abstraction.
- Avoid speculative optimization.
- Prefer stdlib first.
- Keep interfaces and package APIs small.
- Keep code secure at boundaries.
- Keep code maintainable.
