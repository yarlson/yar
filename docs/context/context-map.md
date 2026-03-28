# Context Map

## Core

- [summary.md](summary.md) — Project-wide view of the compiler, pipeline, and implemented capabilities.
- [terminology.md](terminology.md) — Stable terms used by the compiler and runtime contract.
- [practices.md](practices.md) — Enforced invariants and recurring implementation rules.

## Domains

- [domains/compiler-pipeline.md](domains/compiler-pipeline.md) — Internal stage boundaries and package responsibilities.
- [domains/language-slice.md](domains/language-slice.md) — Supported source-level constructs, types, and semantic limits.
- [domains/error-model.md](domains/error-model.md) — Error values, propagation sugar, local handling sugar, and unhandled-error behavior.
- [domains/stdlib.md](domains/stdlib.md) — Embedded standard library: design, infrastructure, packages, and extension guide.

## Flows

- [flows/source-to-native.md](flows/source-to-native.md) — Command-level path from source file to diagnostics, IR, binary, and execution.

## Platform

- [platform/toolchain-runtime.md](platform/toolchain-runtime.md) — External toolchain dependency and embedded runtime boundary.
