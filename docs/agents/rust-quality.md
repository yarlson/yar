# Rust Quality Rules For Agents

Write Rust for readability, correctness, maintainability, security, and
performance, in that order.

Prefer explicit, idiomatic, production-grade code. Do not trade correctness or
clarity for speculative optimization.

## Core Principles

- Prefer simple, idiomatic Rust over cleverness.
- Keep control flow, ownership, and mutation explicit.
- Prefer composition and direct wiring over heavy abstraction.
- Avoid framework-like patterns, unnecessary indirection, and speculative
  design.

## Package Design

- Keep packages small, focused, and acyclic.
- Keep public APIs minimal and hide implementation details by default.
- Prefer shallow package structure and clear boundaries.
- Do not add layers unless they materially improve clarity.

## Traits

- Use concrete types by default.
- Define small traits near the consumer.
- Keep trait bounds explicit and narrow.
- Do not add traits just for future flexibility or mocking.

## Types And API Design

- Make invalid states hard to represent.
- Be explicit about ownership, borrowing, lifetimes, and concurrency behavior.
- Keep APIs small, direct, and hard to misuse.
- Use constructors and abstraction only when they add clear value.

## Naming

- Use clear, domain-specific, descriptive names.
- Prefer intent-revealing names over vague buckets like `util`, `helper`, or
  `manager`.
- Keep package names short and idiomatic.

## Functions And Methods

- Keep functions small, cohesive, and easy to scan.
- Prefer straightforward control flow and early returns.
- Be explicit about mutation, ownership, borrowing, and receiver choice.
- Do not extract helpers that make the call site harder to understand.

## Error Handling

- Handle errors explicitly and never ignore them without a clear reason.
- Add context to errors when it helps callers understand the failure.
- Do not use `panic` for normal error handling.
- Validate inputs and make edge cases explicit at boundaries.

## Cancellation And Process Context

- Pass explicit cancellation, timeout, or configuration values when behavior
  needs them.
- Do not hide process-global assumptions in low-level APIs.
- Propagate caller-controlled limits through request boundaries.
- Respect cancellation, deadlines, and timeouts where they exist.

## Concurrency

- Do not add concurrency unless it is needed and beneficial.
- Prefer simple synchronization and data flow.
- Be explicit about concurrent-safety guarantees.
- Avoid leaks, races, deadlocks, and hidden shared mutable state.

## Performance

- Measure before optimizing.
- Prefer simple algorithms and data structures.
- Avoid unnecessary allocations, copies, conversions, reflection, and boxing.
- Do not trade maintainability for hypothetical speedups.

## Security

- Treat all external input as untrusted.
- Validate, sanitize, and bound data at system boundaries.
- Use secure defaults and least privilege.
- Never log secrets or introduce avoidable data-exposure risks.

## State And Configuration

- Minimize global state and hidden runtime coupling.
- Prefer explicit dependencies and explicit configuration.
- Keep initialization obvious and remove speculative extension points.

## Comments And Documentation

- Write comments only when they add signal.
- Explain why or document non-obvious invariants and tradeoffs.
- Keep comments and exported docs accurate.

## Testing

- Test behavior, edge cases, failure paths, and regressions.
- Prefer deterministic tests and control time, randomness, filesystem, process,
  and concurrency effects.
- Choose the highest-value test layer first; test user-visible flows at the
  boundary that best exercises them.
- Add lower-level tests to protect pure logic and isolate failures.
- Avoid brittle tests and duplicated assertions across layers unless each layer
  catches different risks.

## Dependency Management

- Prefer the standard library first.
- Add third-party dependencies only when clearly justified.
- Avoid dependencies that add more abstraction than value.
