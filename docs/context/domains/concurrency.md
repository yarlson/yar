# Concurrency

## Surface

- `taskgroup []R { ... }` is an expression that runs spawned calls
  concurrently and yields a result slice in spawn order.
- `spawn call(...)` is a statement valid only inside the lexical body of a
  `taskgroup`.
- `chan[T]` is a builtin bounded channel type.
- Channel builtins are `chan_new[T](capacity)`, `chan_send(ch, value)`,
  `chan_recv(ch)`, and `chan_close(ch)`.

## Taskgroup Rules

- The taskgroup annotation must be a slice type. Current code uses:
  - `taskgroup []T` for ordinary task results
  - `taskgroup []!T` for per-task errorable results
  - `taskgroup []void` when tasks return no value
- Each `spawn` target must be a call expression whose return shape matches the
  taskgroup element type.
- `spawn` is rejected outside a taskgroup body.
- `spawn` is also rejected inside a function literal nested under a taskgroup
  body. A nested closure may create and use its own inner taskgroup instead.
- `return` is rejected inside a taskgroup body in the current implementation.
- `break` and `continue` may be used for loops nested inside the taskgroup
  body, but may not jump out through an enclosing loop outside the taskgroup.
- Taskgroup bodies execute sequentially, but each `spawn` starts work
  immediately. Tasks may overlap with later statements in the same body.
- The final result slice preserves spawn order, not completion order.

## Error Model Integration

- `taskgroup []!T` produces a slice of first-class errorable values.
- `?` and `or |err| { ... }` operate on those values after indexing or binding,
  not just on direct errorable calls.
- Raw errorable call expressions are still not generally storable or passable;
  they must still be returned directly, propagated, or handled immediately.

## Channels

- `chan[T]` element types may not be `void`, `noreturn`, or another `chan[U]`.
- Channels are identity-comparable with `==` and `!=`.
- `chan_send` returns `!void` and uses `error.Closed` when the channel has
  been closed.
- `chan_recv` returns `!T` and uses `error.Closed` when receiving from a
  closed and drained channel.
- `chan_close` closes the channel and is non-errorable.
- Channels are FIFO and bounded; send blocks while the buffer is full and recv
  blocks while it is empty and still open.

## Runtime Notes

- The current implementation uses native POSIX threads, not the M:N scheduler
  explored in the original proposal.
- Taskgroup and channel helpers live in `internal/runtime/runtime_source.txt`
  and are linked into every native build.
- Non-Windows builds pass `-pthread` during linking.
- Windows currently has runtime stubs that fail with a clear
  "concurrency is not supported on windows yet" runtime error.
- The conservative GC now guards heap metadata with a mutex and suppresses
  collection while concurrent worker tasks are active.
