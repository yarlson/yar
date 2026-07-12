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
- A spawn target must be a named function or an immediately called inline
  function literal. Arbitrary function values, builtins, and methods are not
  valid spawn targets.
- A direct host-intrinsic spawn also needs a task wrapper. Only `fs.read_file`
  currently has one; other host calls can be made from an inline literal.
- Spawn arguments and inline-literal captures must be share-safe. Scalars,
  `str`, and `error` are share-safe. Fixed arrays, enums, and non-resource
  structs are share-safe only when every contained value is share-safe.
  `!T` and `chan[T]` are share-safe only when `T` is share-safe; `!void` is
  also share-safe.
- Pointers, slices, maps, interfaces, function values, resource structs, and
  aggregates containing any of them cannot cross a spawn boundary.
- Spawn result types are unrestricted by share-safety because results become
  visible to the parent only after the taskgroup joins.
- `spawn` is rejected outside a taskgroup body.
- `spawn` is also rejected inside a function literal nested under a taskgroup
  body. A nested closure may create and use its own inner taskgroup instead.
- `return` is rejected inside a taskgroup body in the current implementation.
- `?` is rejected at the taskgroup body's current function-literal depth
  because propagation would return before the taskgroup join. A nested
  function literal may use `?` for its own errorable return.
- `break` and `continue` may be used for loops nested inside the taskgroup
  body, but may not jump out through an enclosing loop outside the taskgroup.
- Taskgroup bodies execute sequentially, but each `spawn` starts work
  immediately on one native thread. Tasks may overlap with later statements in
  the same body.
- The final result slice preserves spawn order, not completion order.

## Value Boundary

- Spawn arguments and captures use the ordinary calling convention, so the
  task receives shallow value copies rather than deep copies.
- The checker applies the share-safety rule transitively to prevent those
  copies from carrying mutable aliases into another thread. Channels are the
  explicit exception because their runtime operations synchronize shared use.
- Bare `i64` values are treated as scalars. The checker cannot distinguish an
  ordinary integer from a raw runtime or OS handle represented as `i64`, so
  such handles are not rejected by this boundary.
- Runtime-backed `i64` handles are validated against a kind-tagged registry and
  their mutable state is synchronized. This prevents handle-derived invalid
  dereferences and serializes registered state, but does not provide
  compile-time handle provenance or make raw handles part of the share-safe
  source model.

## Error Model Integration

- `taskgroup []!T` produces a slice of first-class errorable values.
- After the taskgroup expression has joined, `?` and `or |err| { ... }` operate
  on its errorable results after indexing or binding, not just on direct
  errorable calls.
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

- The current implementation creates one native POSIX thread immediately for
  each successful `spawn`; it does not use a thread pool or M:N scheduler.
- Taskgroup and channel helpers live in `crates/yar-runtime` for native build
  paths.
- Runtime bundles carry the target's ordered native-library requirements;
  Linux GNU bundles include `pthread`, while Darwin uses its own Rust
  static-library contract.
- Windows currently has runtime stubs that fail with a clear
  "concurrency is not supported on windows yet" runtime error.
- The collector defers marking and sweeping while any spawned task result is
  unjoined. Worker allocations remain registered, and collection resumes only
  after every outstanding result has been joined and copied into managed
  storage.
- Live channel buffer slots are explicit conservative roots because channel
  storage lives outside the managed heap. Consumed slots are cleared.
- Explicit file and network close removes the handle from the runtime registry;
  string-builder handles have no close operation and remain registered for the
  process lifetime.
