# AGENTS.md

This file is the mandatory entrypoint for agent work in this repository.

Use progressive disclosure:

1. Read this file.
2. Read `docs/context/summary.md` and `docs/context/practices.md`.
3. Read only the relevant detailed agent guide or guides:
   - `docs/agents/development-workflow.md` for any code, docs, testdata, CI,
     release, or tooling change.
   - `docs/agents/language-design.md` for language surface, stdlib API,
     runtime boundary, proposal, roadmap, or language-process work.
   - `docs/agents/rust-quality.md` for Rust implementation details.
4. Read the specific `docs/context/` and `docs/language/` files touched by the
   task.

Do not treat old session memory, roadmap text, proposal text, or stale docs as
truth until the live checkout confirms it.

## Core Working Rules

- Define success criteria before editing.
- Plan the implementation and verification approach before changing files.
- Make surgical changes only.
- Preserve unrelated user changes.
- Prefer the simplest solution that fully solves the task.
- Do not add abstractions, configurability, or feature surface unless the task
  requires it.
- Update tests, `testdata/`, docs, and tooling when behavior or accepted
  language surface changes.
- Verify every meaningful change with concrete commands.
- Run a review pass before finalizing implementation work.

The Rust frontend owns accepted YAR syntax. External Tree-sitter and JetBrains
repositories own their grammar projections, generated artifacts, tests, and
releases; do not copy those artifacts into this repository. Keep
`testdata/syntax_surface` aligned with syntax changes, then leave projection
delivery to its owning repository.

## Language Direction Rules

- YAR is a small native language with explicit control flow, explicit errors,
  a Rust 2024 implementation, LLVM IR generation, and a Rust runtime linked
  through a stable C ABI.
- Errors are values. Do not introduce exception-like semantics, hidden stack
  unwinding, or `try`/`catch`-style language direction.
- Prefer familiar syntax unless there is a proven YAR-specific reason to do
  something custom.
- If an API surface starts looking ugly or over-layered, step down to the
  missing language/runtime/stdlib substrate instead of piling helpers on top.
- Runtime owns raw allocation, GC, collection layout, channels, taskgroups,
  OS handles, syscalls, and ABI shims. Stdlib owns user-facing API shape,
  deterministic policy, and composition expressible in YAR.
- Public stdlib or routing design should be grounded in current ecosystem
  practice when the user asks for that or when the surface is unfamiliar.

## Source-Of-Truth Rules

Documentation ownership is defined in `docs/language/process.md`:

- code and executable tests are behavioral authority
- `docs/YAR.md` owns current public behavior
- `docs/context/` owns current internal architecture and operations
- `LLM.txt` is a derived compact mirror
- proposals own design and implementation evidence
- proposal metadata is synchronized into `docs/language/README.md`
- `docs/language/decisions.md` owns design rationale
- `docs/language/roadmap.md` contains future planning only

Update only the surfaces whose owned truth changed. Also update `README.md` for
public usage, `testdata/` for representative programs, and syntax tooling when
those contracts change. If owned sources disagree, repair them as part of the
task or call out why the correction is deliberately out of scope.

## Required Verification

For Rust implementation changes, run:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/verify-rust-testdata.sh
./scripts/verify-rust-testdata-run.sh
```

Before finalizing, run the check-form variants:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/verify-rust-testdata.sh
./scripts/verify-rust-testdata-run.sh
```

Use focused checks first when they shorten feedback, but do not replace the
required final gates for implementation changes.

For docs-only changes, run at least `git diff --check` and inspect the rendered
or changed Markdown enough to catch broken structure.

## Final Self-Check

Before finalizing, verify:

- The requested task is fully handled.
- The diff is scoped to the request.
- The implementation follows the accepted language/process direction.
- Tests or concrete checks support the change.
- Source-of-truth docs are synchronized or explicitly called out.
- No unrelated files were reverted or cleaned up.
