# Development Workflow For Agents

Use this guide for code, docs, fixtures, tooling, CI, and release work.

## Work Classification

Before editing, classify the task:

- language surface or semantics
- compiler frontend, checker, lowering, or LLVM emission
- runtime or ABI helper
- stdlib package
- CLI/build/test/release workflow
- docs-only truth sync
- external tooling such as tree-sitter or JetBrains plugin

Then read the relevant `docs/context/` domain files and any relevant
`docs/language/proposals/*.md` files.

## Success Criteria

State concrete success criteria before implementation. They should include:

- behavior to add, remove, or preserve
- tests or fixtures that prove it
- docs and source-of-truth files that must change
- verification commands to run
- explicit out-of-scope items

For bug fixes, reproduce the bug first when practical. If reproduction is not
practical, state why and use the closest concrete check.

## Implementation Order

For implementation work:

1. Read `docs/context/summary.md`, `docs/context/practices.md`, and relevant
   context/domain files.
2. If language or stdlib API behavior changes, read
   `docs/agents/language-design.md`.
3. Plan implementation and test approach.
4. Write or update tests first when practical.
5. Implement the smallest complete slice.
6. Update `testdata/` with durable representative programs when behavior,
   fixtures, or accepted language surface changes.
7. Run focused checks.
8. Update source-of-truth docs.
9. Run the required full verification gates from `AGENTS.md`.
10. Review the diff for correctness, maintainability, boundary drift, stale docs,
    and missing tests.
11. Fix review findings and rerun affected checks.

## Testdata Rules

`testdata/` is part of the language contract.

- Add fixtures for user-visible behavior, not incidental parser trivia.
- Keep fixtures isolated under their own directories when they declare
  `package main`.
- Prefer one durable fixture per capability or regression.
- Cover failure behavior with automated tests at the compiler/checker layer
  when a fixture cannot naturally run as a success program.
- Update fixture verification scripts only when the fixture class changes.

## Documentation Rules

Do not leave implementation truth split across stale docs.

Update the smallest necessary set of:

- `docs/context/` for current internal architecture and operations
- `docs/YAR.md` for current public language and API behavior
- `LLM.txt` as the derived compact mirror of current behavior
- `docs/language/decisions.md` for accepted, rejected, deferred, or withdrawn
  rationale
- proposal metadata and evidence when design or delivery state changes
- `docs/language/README.md` to keep its proposal registry synchronized
- `docs/language/roadmap.md` for future planning only
- `README.md` for user-facing capability or usage changes

Roadmap and proposal files are not current behavioral authority. If an owned
documentation surface disagrees with code and executable tests, correct it.

## Tooling Rules

Syntax changes can break developer tooling even when the compiler is green.
Keep the recursive tracked `*.yar` contract under `testdata/syntax_surface`
aligned with every accepted syntax change.

Tree-sitter and JetBrains repositories independently own their grammars,
generated artifacts, editor behavior, tests, compatibility metadata, and
releases. Do not vendor those projections here. When syntax, keywords,
declarations, literals, or diagnostics change, hand the updated fixture and YAR
revision to those owners and check whether these are in scope there:

- tree-sitter grammar and highlight queries
- JetBrains plugin parser/highlighting/annotator
- README install or validation instructions
- release artifacts or packaged runtime layout

If tooling is out of scope, mention the expected follow-up explicitly.

## Review Rules

Review from the real diff, not memory.

- Read changed files in full where behavior is non-trivial.
- Trace related call sites and contracts.
- Look for semantic drift, missing tests, stale docs, boundary leaks, and
  runtime/ABI assumptions.
- Treat a green test suite as evidence, not as a substitute for review.
