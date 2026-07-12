# AGENTS.md

## Implementation Process

For implementation work in this repository, follow this order:

1. read `docs/context/`
2. plan implementation and the test approach before changing code
3. write or update tests first when practical; prefer TDD for new behavior, bug fixes, and regressions
4. implement
5. update `testdata/` meaningfully and reasonably when behavior, fixtures, or representative programs change; syntax changes must keep the portable accepted-syntax fixture under `testdata/syntax_surface` aligned
6. run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `./scripts/verify-rust-testdata.sh`, and `./scripts/verify-rust-testdata-run.sh`
7. fix issues from lint and tests
8. update affected `docs/context/` files when internal architecture or
   operations changed
9. update `docs/YAR.md` when public language/API behavior changed, then update
   the derived `LLM.txt` mirror
10. update `docs/language` when design status, rationale, proposal evidence,
    delivery state, process, or future planning changed
11. use `/review` slash command from `~/.claude/commands/review.md` or `review` skill to review the code
12. fix all found issues
13. run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `./scripts/verify-rust-testdata.sh`, and `./scripts/verify-rust-testdata-run.sh`
14. fix issues from lint and tests
15. if needed, repair the affected owned documentation surfaces

Documentation ownership is defined in `docs/language/process.md`: code and tests
are behavioral authority, `docs/YAR.md` owns current public behavior,
`docs/context/` owns current internal behavior, `LLM.txt` is derived, proposals
preserve design/evidence, decisions preserve rationale, and the roadmap is
future-only. Do not update unrelated documentation merely to touch every layer,
and do not skip the surfaces whose owned truth changed.

Do not skip test work for implementation changes. Add or update automated tests at
the highest-value layer for the change, and keep `testdata/` focused on
representative, durable fixtures rather than incidental cases.

The Rust frontend owns accepted YAR syntax. External Tree-sitter and JetBrains
repositories own their grammar projections, generated artifacts, tests, and
releases; do not copy those artifacts into this repository. Record the syntax
change and portable fixture here, then leave projection delivery to its owning
repository.

## Rust Code Quality Standard

Write Rust for readability, correctness, maintainability, security, and
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

- Prefer simple, idiomatic Rust over cleverness.
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

## Traits

- Use concrete types by default.
- Define small traits near the consumer.
- Keep trait bounds explicit and narrow.
- Do not add traits just for future flexibility or mocking.

---

## Types and API Design

- Make invalid states hard to represent.
- Be explicit about ownership, borrowing, lifetimes, and concurrency behavior.
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
- Be explicit about mutation, ownership, borrowing, and receiver choice.
- Do not extract helpers that make the call site harder to understand.

---

## Error Handling

- Handle errors explicitly and never ignore them without a clear reason.
- Add context to errors when it helps callers understand the failure.
- Do not use `panic` for normal error handling.
- Validate inputs and make edge cases explicit at boundaries.

---

## Cancellation and Process Context

- Pass explicit cancellation, timeout, or configuration values when behavior
  needs them.
- Do not hide process-global assumptions in low-level APIs.
- Propagate caller-controlled limits through request boundaries.
- Respect cancellation, deadlines, and timeouts where they exist.

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

Before finalizing a Rust change, verify readability, correctness on edge cases,
explicit error handling, justified abstractions, safe boundary validation,
necessary and safe concurrency, reasonable performance, explicit security
handling, minimal global state, and maintainability.

---

## Hard Rules

- Prefer simple, explicit code.
- Prefer concrete types and narrow traits.
- Handle errors explicitly.
- Pass cancellation, timeout, and configuration explicitly when needed.
- Avoid global state.
- Avoid premature abstraction.
- Avoid speculative optimization.
- Prefer stdlib first.
- Keep interfaces and package APIs small.
- Keep code secure at boundaries.
- Keep code maintainable.
