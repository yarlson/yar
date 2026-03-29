# Proposal: Host Process and Environment

Status: implemented

## 1. Summary

Add the minimum host process surface needed for a self-hosted compiler CLI.

The proposed packages are:

- `process` for command-line arguments and child-process execution
- `env` for environment lookup
- `stdio` for stderr output

The minimal API is:

- `process.args() []str`
- `process.run(argv []str) !process.Result`
- `process.run_inherit(argv []str) !i32`
- `env.lookup(name str) !str`
- `stdio.eprint(msg str) void`

```
struct Result {
    exit_code i32
    stdout str
    stderr str
}
```

## 2. Motivation

Filesystem access is enough to self-host package loading, but not enough to
self-host the command-line tool around it.

A self-hosted `yar` compiler eventually needs to:

- read command-line arguments
- honor environment overrides such as `CC`
- invoke external tools such as `clang`
- run built programs with inherited stdio
- print diagnostics to stderr without treating them as panics

Today the Go driver owns all of that host process logic. Without a corresponding
yar surface, a rewritten frontend can at best become a library embedded inside a
permanent Go CLI.

## 3. User-Facing Examples

### Valid examples

```
import "process"

fn main() i32 {
    args := process.args()
    if len(args) < 2 {
        return 2
    }
    return 0
}
```

```
import "env"

fn cc_name() str {
    cc := env.lookup("CC") or |err| {
        return "clang"
    }
    return cc
}
```

```
import "process"

fn link(ir_path str, runtime_path str, out_path str, cc str) !i32 {
    result := process.run([]str{
        cc,
        "-Wno-override-module",
        ir_path,
        runtime_path,
        "-o",
        out_path,
    })?
    return result.exit_code
}
```

```
import "stdio"

fn report(msg str) void {
    stdio.eprint(msg)
}
```

### Invalid examples

```
result := process.run("clang")
```

Invalid because child-process execution takes `[]str`, not one flat command
string.

```
name := env.lookup(1)?
```

Invalid because environment names are strings.

```
code := process.args()?
```

Invalid because `process.args()` is not errorable.

## 4. Semantics

This proposal adds a small host process boundary.

### `process.args`

- returns the full argument vector as `[]str`
- index `0` is the executable name when the host provides it
- argument values are copied into ordinary YAR strings and slices

### `process.run`

- launches one child process from `argv`
- `argv[0]` names the executable to launch
- captures child stdout and stderr into the returned `Result`
- returns `error.NotFound`, `error.PermissionDenied`, `error.InvalidArgument`,
  or `error.IO` only when process launch or host coordination fails
- a non-zero child exit status is not a YAR `error`; it is represented by
  `Result.exit_code`

This separation matters because compilers often need to inspect failed child
results rather than treating them as host failures.

### `process.run_inherit`

- launches one child process with inherited stdin, stdout, and stderr
- returns the child exit code on successful launch and completion
- is useful for the eventual `yar run` path

### `env.lookup`

- returns the value of one environment variable
- returns `error.NotFound` when absent

### `stdio.eprint`

- writes a string to stderr
- does not return an error in the first version

The first version intentionally avoids a general streaming I/O model. The
immediate self-hosting need is compiler diagnostics and child-process
coordination, not an open-ended descriptor API.

## 5. Type Rules

- `process.args()` returns `[]str`
- `process.run(argv)` requires `argv` to be `[]str` and returns `!process.Result`
- `process.run_inherit(argv)` requires `argv` to be `[]str` and returns `!i32`
- `env.lookup(name)` requires `name` to be `str` and returns `!str`
- `stdio.eprint(msg)` requires `msg` to be `str` and returns `void`
- raw errorable host-process calls remain subject to the ordinary error rules

## 6. Grammar / Parsing Shape

No new grammar is required.

This proposal is entirely library-shaped:

- `process.args()`
- `process.run(argv)`
- `env.lookup("CC")`
- `stdio.eprint(msg)`

## 7. Lowering / Implementation Model

- parser: no changes
- AST / IR: no new node kinds
- checker: ordinary package-qualified function calls, with selected embedded
  stdlib declarations tagged as host intrinsics during package-loaded signature
  registration
- codegen: lower host-bound calls to runtime/ABI shims
- runtime: add argument retrieval, environment lookup, child-process launch,
  output capture, inherited stdio execution, and stderr write support

As with filesystem access, these packages are standard-library in user shape but
host-backed in implementation.

The key lowering rule is:

- host launch failure becomes a YAR `error`
- child exit status becomes data in `Result.exit_code`

Additional implementation constraints from proposal 0009:

- keep the user-facing API package-shaped rather than introducing a new family
  of syntax-level host builtins
- prefer ABI-stable runtime entry points such as `status + out-parameter`
  signatures for larger aggregate results instead of relying on direct
  aggregate returns across all targets
- preserve stable user-visible YAR error names even if the runtime uses
  implementation-specific host status codes internally
- keep as much deterministic logic as possible in yar source, reserving runtime
  shims for irreducible host interaction only

## 8. Interactions

- errors: integrates directly with the existing explicit error model
- structs: `process.Result` is an ordinary struct
- arrays: no special interaction
- control flow: child-process failure stays explicit and inspectable
- returns: `Result` and exit codes return like normal values
- builtins: no new syntax-level process builtin is required
- future modules/imports: a self-hosted CLI depends on args, env, and process
  execution
- future richer type features: later streaming or binary I/O can extend this
  boundary without changing the minimal self-hosting story

## 9. Alternatives Considered

### Keep process execution in an outer Go launcher forever

Rejected because that would make the self-hosted compiler permanently dependent
on a privileged non-yar shell around it.

### Treat non-zero child exit as `error`

Rejected because build tools often need the child exit code and captured stderr
as data rather than as a collapsed failure path.

### Add a full shell language interface

Rejected because shell parsing, pipelines, quoting, and redirection would add
far too much surface. An explicit `[]str` argv model is smaller and clearer.

## 10. Complexity Cost

- language surface: medium
- parser complexity: low
- checker complexity: low
- lowering/codegen complexity: medium
- runtime complexity: high
- diagnostics complexity: medium
- test burden: high
- documentation burden: high

## 11. Why Now?

Once filesystem access exists, the next blocker for a true self-hosted compiler
CLI is the process boundary: arguments in, diagnostics out, toolchain
subprocesses around the generated IR.

This proposal keeps that boundary explicit and intentionally small.

## 12. Open Questions

- Should `process.run` grow a working-directory parameter in the first version,
  or remain argv-only?
- Should `stdio.eprint` stay infallible, or should stderr writes eventually
  return `!void` like other host operations?
- Should `env.lookup` be accompanied by `env.has` or `env.value_or` helpers, or
  is the ordinary error model sufficient?
- Should `process.Result` include a `success bool`, or is `exit_code == 0`
  enough?
- Should the first runtime implementation be explicitly POSIX-oriented, with a
  later Windows-specific layer, or should cross-platform parity be required in
  the first cut?
- Which process/environment entry points should use direct returns versus
  explicit out-parameters to keep the runtime ABI robust across targets?

## 13. Decision

Proposed.

This belongs in the self-hosting proposal set because it defines the host
process boundary around a self-hosted compiler, not just one isolated library
convenience.

## 14. Implementation Checklist

- stdlib package API design
- runtime process and environment boundary
- lowering/codegen hooks
- checker support for tagging selected embedded stdlib declarations as host
  intrinsics
- ABI design for runtime shims, including out-parameter shapes where needed
- diagnostics for launch failures and environment lookup failures
- stable mapping from runtime host statuses to user-visible YAR error names
- integration tests for args, stderr, and subprocesses
- ABI-sensitive tests on supported targets for aggregate-returning APIs
- CLI bootstrap plan
- `current-state.md` update
- `decisions.md` update
