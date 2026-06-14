# Language Design Rules For Agents

Use this guide for language surface, stdlib API, runtime boundary, proposal,
roadmap, and design-process work.

## Stable Direction

YAR is evolving toward a small native language that can write real tools and,
eventually, more of its own frontend.

Preserve these identity points:

- explicit errors as values
- visible control flow
- Go-like readability and directness
- conventional syntax when convention is good enough
- compiler-owned type and runtime contracts
- stdlib APIs written in YAR where policy can be expressed in YAR
- native execution through LLVM IR and the Rust runtime

Do not add exception-like behavior, hidden control flow, macro systems,
implicit conversions, framework-style stdlib layers, or clever syntax without
strong evidence.

## Feature Gate

No meaningful language feature should move forward without:

- a real pressure point from current programs, fixtures, stdlib, self-hosting,
  or tooling
- a written proposal or explicit update to an existing proposal
- valid examples and invalid examples
- interactions with errors, generics, methods, interfaces, closures, packages,
  stdlib, runtime, and tooling considered where relevant
- a test plan
- source-of-truth docs identified up front

Small fixes may be direct, but semantic changes need the gate.

## Capability Milestones

Plan by newly enabled programs, not by isolated syntax.

Good milestone question:

What useful program becomes easier or newly possible after this change?

Examples from prior work:

- self-hosting preparation needed filesystem, process/env, map keys, and sort
- HTTP routing first needed a streaming/resource substrate
- native services needed TCP, concurrency, errors, maps, and stdlib wrappers

If a proposal cannot answer the useful-program question, defer it.

## API Design

Keep public stdlib APIs small and explicit.

- Prefer concrete structs, functions, and interfaces already supported by YAR.
- Avoid middleware, extractor, framework, or magic registration layers unless
  the language has real pressure for them.
- Avoid names that collide with builtins such as `delete`.
- When the user asks for external grounding, research current ecosystem
  practice before proposing API shape.
- If the design feels awkward, do not paper it over with helpers. Identify the
  missing language/runtime/stdlib capability.

## Runtime And Stdlib Boundary

Use this split:

- Runtime owns raw allocation, GC, maps, channel/taskgroup internals, string
  layout, OS handles, syscalls, blocking primitives, ABI shims, and platform
  status translation.
- Compiler/codegen owns lowering from checked YAR declarations to runtime ABI
  calls.
- Stdlib owns user-facing API shape, deterministic path/text/policy logic,
  wrappers, composition, and interfaces expressible in YAR.

Do not move behavior into stdlib merely to shrink the runtime if the behavior
needs raw memory, compiler-owned layout, OS handles, or ABI details.

Good candidates for stdlib are deterministic policy and composition. Poor
candidates are raw syscalls, allocator behavior, GC, maps, channels,
taskgroups, and platform ABI differences.

## Truth Sync

Language work is not done until the affected owned surfaces agree. Code and
executable tests are behavioral authority; `docs/YAR.md` owns current public
behavior, `docs/context/` owns current internal architecture, and `LLM.txt` is
derived. Proposals own design and implementation evidence, with their metadata
synchronized into `docs/language/README.md`; decisions own rationale and the
roadmap owns future planning only. Update `testdata/` and external syntax
tooling when their contracts are affected.

If implementation made an owned surface stale, do a truth-sync pass before
adding more feature surface.

## Rejected Directions

Do not reintroduce:

- `try` / `catch` as the primary error model
- hidden exception-like flow
- custom pointer allocation syntax when `&` / `*` is enough
- high-level public APIs that exist only because the substrate is missing
- broad runtime-to-stdlib moves without a boundary plan
