# Proposal: Host Process and Environment

Status: accepted
Implementation: implemented

## 1. Summary

The embedded `process`, `env`, and `stdio` packages provide the host boundary
needed by compiler-style programs without exposing a shell language or raw
process handles.

The process API is explicit about lifetime and capture bounds:

```yar
process.args() []str
process.limits(timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64) !process.Limits
process.cancellation() process.Cancellation
process.cancel(value process.Cancellation) void
process.run(argv []str, limits process.Limits, cancellation process.Cancellation) !process.Result
process.run_inherit(argv []str, timeout_milliseconds i64, cancellation process.Cancellation) !i32
```

`env.lookup(name str) !str` reads one environment value, and
`stdio.eprint(msg str) void` writes one complete stderr message.

## 2. Types

```yar
pub struct Result {
    pub exit_code i32
    pub stdout str
    pub stderr str
}

pub struct Limits {
    timeout_milliseconds i64
    max_stdout_bytes i64
    max_stderr_bytes i64
}

pub struct Cancellation {
    signal chan[bool]
}
```

`Cancellation` is a share-safe, close-only signal. Copies refer to the same
signal. `cancel` is idempotent and never sends or consumes a channel value.
`Limits` and `Cancellation` have private fields and are constructed through
`limits(...)` and `cancellation()`; `Result` exposes its output fields.

## 3. Process Arguments

`process.args()` returns the full host-provided argument vector as copied Yar
strings. Index zero is the executable name when the host provides it.

Child argv is passed directly without shell parsing or interpolation.
`argv[0]` must be non-empty, and every argument must cross the host boundary
without an embedded NUL. The child inherits the caller's working directory and
environment. Captured `run` uses null stdin; `run_inherit` inherits stdin.

## 4. Limits and Cancellation

`process.limits` validates all three values:

- timeout: 1 through 86,400,000 milliseconds (24 hours)
- stdout cap: 0 through 67,108,864 bytes (64 MiB)
- stderr cap: 0 through 67,108,864 bytes (64 MiB)

A zero capture cap allows exactly zero bytes; it does not mean unlimited.
`process.run` validates `Limits` again as a host-boundary defense and because
zero-value or aggregate initialization can still produce invalid `Limits`
values even though its fields are now private.

The cancellation signal is checked before launch. After the host creates and
contains the child, the deadline and cancellation signal cover execution,
concurrent pipe draining, leader wait, and ordinary descendants that retain a
capture pipe. Portable process creation is a synchronous host call: a stalled
executable lookup or host spawn operation cannot be interrupted until that call
returns. This limitation does not give the child extra execution time after a
successful launch.

The exact configured cap is allowed. The first byte beyond either cap,
deadline expiry, or cancellation terminates the contained process tree. The
runtime closes/drains capture readers as needed, waits for termination, and
reaps the leader before returning. Captured partial output is discarded.

The resulting errors are:

- `process.Timeout` for deadline expiry
- `process.LimitExceeded` for either capture cap
- `process.Cancelled` for an explicit cancellation signal
- `process.IO` when termination, cleanup, capture, or waiting cannot complete

Cleanup failure takes precedence over the trigger error because the runtime
cannot claim successful containment when cleanup is incomplete.

## 5. `process.run`

`run` captures stdout and stderr concurrently so one full pipe cannot deadlock
the other. Caps count raw bytes independently. Successful completion returns
the complete captured byte strings and the leader exit status. A non-zero exit
status remains data in `Result.exit_code`, not a Yar error.

The call blocks only its calling native Yar task thread. Sibling taskgroup
threads continue to run and may cancel the shared `Cancellation` value.

## 6. `process.run_inherit`

`run_inherit` inherits stdin, stdout, and stderr, so capture caps do not apply.
Its timeout uses the same 1-through-24-hour range, and its cancellation and
cleanup semantics match `run`. Successful completion returns the child exit
status as `i32`.

## 7. Containment Boundary

On Unix, controlled children start in a new process group. Timeout, capture
limit, or cancellation terminates that group and reaps the leader. A descendant
that deliberately creates a new session can escape this boundary.

On Windows, the child is assigned to a kill-on-close Job Object before it is
resumed. Cleanup waits until the job reports no active processes.

This contract controls wall-clock lifetime and captured byte volume. It is not
a sandbox and does not impose CPU, address-space, file, network, or process-count
quotas.

## 8. Other Host Operations

`env.lookup` returns `env.NotFound` when a name is absent and
`env.InvalidArgument` when a name cannot cross the host boundary.
`stdio.eprint` serializes one complete message with other runtime stderr calls.

## 9. CLI Boundary

Source-level process limits are function arguments. They are independent of
the Rust CLI's build, test, and Git operation deadlines. CLI timeout environment
variables neither configure nor override `std/process`, and `yar run` does not
apply its build deadline to the user program.

## 10. Implementation Model

The package surface lowers to ABI-stable runtime entry points using status plus
out-parameters for aggregate results. Host launch and coordination failures map
to stable Yar error names. Concurrent bounded readers prevent pipe deadlock,
and platform containment keeps ordinary descendants within cleanup scope.

No grammar or syntax-level process builtin is required.

## 11. Errors

Process execution may produce `NotFound`, `PermissionDenied`,
`InvalidArgument`, `Timeout`, `LimitExceeded`, `Cancelled`, or `IO`.
Environment lookup contributes `NotFound`, `PermissionDenied`,
`InvalidArgument`, or `IO` independently.

## 12. Verification Contract

Tests cover validation before spawn, direct argv behavior, inherited cwd and
environment, exact capture boundaries, concurrent stdout/stderr draining,
captured and inherited timeout behavior, pre-spawn and in-flight cancellation,
ordinary descendant containment, and native Unix and Windows process-control
paths. The native Yar fixture exercises the public API while collection is
forced to a low threshold. CLI subprocess tests independently retain their own
timeout and signal-forwarding contract; source limits come only from explicit
Yar arguments.
