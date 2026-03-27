# AGENTS.md

## Go Code Quality Standard

All Go code in this repository must be written for:

- correctness
- readability
- maintainability
- security
- performance

Priority order:

1. readability
2. correctness
3. maintainability
4. performance

Do not trade correctness or clarity for speculative optimization.

The goal is production-grade Go that is boring in the best possible way: explicit, predictable, easy to debug, easy to change, and idiomatic.

---

## Core Principles

- Prefer simple, idiomatic Go over cleverness.
- Follow standard Go idioms, Effective Go, Go Proverbs, and the style of the standard library.
- Keep code easy to scan and easy to reason about.
- Prefer explicitness over magic.
- Prefer composition over deep embedding or inheritance-like patterns.
- Prefer manual dependency injection and explicit wiring.
- Avoid framework-like overengineering.
- Avoid premature abstraction.
- Avoid unnecessary indirection.
- Keep the common path easy and the wrong path hard.

---

## Package Design

- Use small, focused packages with clear responsibilities.
- Keep package boundaries clean.
- Avoid circular dependencies.
- Keep public APIs minimal.
- Hide implementation details unless there is a clear reason to expose them.
- Prefer shallow package hierarchies.
- Separate domain logic from transport, storage, HTTP, CLI, and infrastructure concerns.
- Do not create meaningless layers.

---

## Interfaces

- Accept interfaces, return structs.
- Use concrete types by default.
- Return interfaces only when there is a strong and explicit reason.
- Define interfaces close to the consumer, not the producer.
- Keep interfaces very small and behavior-focused.
- Do not introduce interfaces for future possibilities or mocking alone.
- Avoid giant interfaces and god abstractions.

Good:

- consumer-owned small interfaces
- minimal behavior contracts
- concrete return types

Bad:

- producer-owned interfaces with one implementation
- broad “service” interfaces without clear need
- interface-heavy design that hides straightforward code

---

## Types and API Design

- Make invalid states hard to represent.
- Design types and functions so misuse is difficult.
- Be explicit about ownership, lifecycle, concurrency guarantees, and zero-value behavior.
- Prefer zero-value usability when appropriate.
- Use constructors only when they add real value.
- Keep APIs minimal, intuitive, and hard to misuse.
- Model the domain directly and clearly.
- Preserve backward compatibility when required. Any breaking change must be deliberate and minimal.

---

## Naming

- Name things clearly, precisely, and in domain language.
- Prefer descriptive names over short clever ones.
- Avoid vague names like:
  - `util`
  - `helper`
  - `common`
  - `misc`
  - `manager`
  - `processor`
  - `data`
  - `info`
- Package names should be short, specific, and idiomatic.
- Function and method names should communicate intent, not implementation trivia.

---

## Functions and Methods

- Keep functions small and cohesive.
- Keep control flow straightforward.
- Prefer early returns over deep nesting.
- Avoid long functions with mixed responsibilities.
- Avoid hidden side effects.
- Be explicit about mutation and ownership.
- Choose pointer vs value receivers deliberately.
- Pass large structs carefully.
- Do not introduce helpers that make code less readable.

---

## Error Handling

- Handle errors explicitly.
- Never ignore errors unless it is intentional and provably safe.
- Add context to errors where useful.
- Prefer wrapping with `%w`.
- Do not use `panic` for normal error handling.
- Error messages should be actionable and useful.
- Validate inputs at system boundaries.
- Make edge cases explicit.

---

## Context Usage

- Pass `context.Context` explicitly where appropriate.
- `context.Context` should usually be the first parameter.
- Never store context in structs.
- Respect cancellation, deadlines, and timeouts.
- Propagate context through request and operation boundaries.
- Do not pass `context.Background()` through code paths that already have a caller context unless there is a very strong reason.

---

## Concurrency

- Do not introduce concurrency unless it is needed.
- Concurrency must have a clear, measurable benefit.
- Be explicit about whether a type is safe for concurrent use.
- Avoid goroutine leaks, deadlocks, races, hidden shared mutable state, and unnecessary synchronization.
- Use channels only when they are the clearest tool for the problem.
- Prefer simple synchronization and data flow.
- Avoid concurrency complexity in code that does not need it.

---

## Performance

- Measure before optimizing.
- Do not apply speculative micro-optimizations.
- Prefer simple algorithms and straightforward data structures.
- Be aware of algorithmic complexity, allocation behavior, copies, memory usage, cache behavior, and contention.
- Avoid unnecessary:
  - allocations
  - copies
  - string conversions
  - interface boxing
  - reflection
  - dynamic dispatch in hot paths
- Preallocate only when it materially helps and is justified by usage.
- Keep hot paths obvious and cheap.
- Never sacrifice maintainability for hypothetical performance gains.

---

## Security

- Treat all external input as untrusted.
- Validate, sanitize, and bound input appropriately.
- Fail safely.
- Use secure defaults.
- Apply least privilege.
- Avoid:
  - injection vulnerabilities
  - unsafe deserialization
  - path traversal
  - insecure randomness
  - accidental data leaks
- Prefer standard library and battle-tested approaches.
- Never log secrets, tokens, passwords, or sensitive personal data.
- Make security-relevant behavior explicit.

---

## State and Configuration

- Minimize global state.
- Prefer explicit dependencies over package-level singletons.
- Keep configuration explicit.
- Avoid hidden runtime coupling.
- Make lifecycle and initialization behavior obvious.
- Remove dead code, duplication, and speculative extension points.

---

## Comments and Documentation

- Write comments only when they add signal.
- Explain why, not what, unless the what is genuinely non-obvious.
- Document exported types and functions.
- Document non-obvious invariants, tradeoffs, and performance characteristics.
- Keep comments accurate and updated with the code.

---

## Testing

- Design code for testability.
- Test behavior, invariants, edge cases, and failure modes.
- Add regression tests for bugs.
- Prefer deterministic tests.
- Control time, randomness, filesystem, process, and concurrency effects when possible.
- Prefer table-driven tests where they improve clarity.
- Avoid brittle tests coupled too tightly to implementation details.
- Use benchmarks for performance-sensitive paths when relevant.

Tests should verify:

- normal behavior
- edge cases
- invalid input
- failure handling
- concurrency behavior where applicable
- regressions from past bugs

When choosing a test layer:

- If an API change affects a user-visible or cross-app journey, identify that journey first and start from the highest-value boundary that exercises it.
- Do not begin with package-local unit tests when the main risk is the HTTP contract or the web-to-API flow.
- Prefer integration-style HTTP tests for route behavior, middleware behavior, status codes, headers, and JSON contracts.
- Keep package-local unit tests focused on pure logic such as config parsing, validation, helpers, and small domain behavior.
- Add lower-layer tests after the top boundary test exposes a need to localize failures or to protect pure logic and boundary edge cases.
- Avoid restating the same endpoint contract in unit, integration, and browser E2E tests unless the layers are catching different risks.
- If an API change affects a user-visible flow in `apps/web`, expect the top-level journey to be covered from the browser layer as well.

---

## Dependency Management

- Prefer the standard library first.
- Add third-party dependencies only when clearly justified.
- Minimize dependencies.
- Avoid dependencies that add abstraction without strong value.
- Do not introduce a library when a small amount of clear code is better.

---

## Change Guidelines

When generating or editing code:

- preserve existing style if it is already good and idiomatic
- improve naming, structure, and boundaries where necessary
- do not introduce abstractions without justification
- do not add files, layers, or patterns unless they clearly improve the code
- call out tradeoffs when multiple valid approaches exist
- explain changes to performance-sensitive code
- explain changes to security-sensitive code
- prefer minimal, high-signal changes over broad rewrites unless a rewrite is clearly warranted

---

## Review Checklist

Before finalizing any Go change, verify:

- Is the code idiomatic Go?
- Is it easy to read quickly?
- Is it correct on edge cases?
- Is input validated at boundaries?
- Is error handling explicit and strong?
- Are abstractions justified?
- Are interfaces minimal and defined near the consumer?
- Are concrete types returned where possible?
- Is concurrency necessary and safe?
- Is the performance reasonable and based on actual need?
- Is security handled explicitly?
- Is global state avoided?
- Is the code maintainable by another engineer in six months?

---

## Hard Rules

- Prefer simple, explicit code.
- Accept interfaces, return structs.
- Keep interfaces small and local to the consumer.
- Keep package APIs small.
- Handle errors explicitly.
- Pass context explicitly.
- Avoid global state.
- Avoid premature abstraction.
- Avoid speculative optimization.
- Prefer stdlib first.
- Keep code secure at boundaries.
- Keep code maintainable
  .
