# Proposal: Structured Concurrency (`taskgroup` and `chan`)

Status: accepted
Implementation: implemented

## 1. Summary

Add structured concurrency to Yar through two constructs: `taskgroup` blocks
that spawn and join concurrent tasks with guaranteed lifetime, and typed bounded
channels for inter-task communication. The current implementation starts each
spawn on a native OS thread and joins through the taskgroup runtime API.
This preserves the surface semantics proposed here while deferring the
exploratory M:N scheduler work. Arguments and closure captures use shallow
value copies, so the checker admits only transitively share-safe values at the
task boundary. This composes with `!T` error returns for explicit error
handling across task boundaries and introduces no function coloring. Blocking
operations (I/O, channels, and other host calls) block the spawned task's
native thread, not other task threads.

## 2. Motivation

Before this proposal, Yar was single-threaded. All blocking calls (networking,
sleep, file I/O) halted the entire program. This prevented:

- **Concurrent servers**: a TCP server cannot handle more than one connection
  at a time because `net.accept` and `net.read` block the program.
- **Parallel computation**: CPU-bound work cannot use multiple cores.
- **Background work with I/O overlap**: a program cannot download a file while
  processing another.
- **Timed operations**: there is no way to race a timeout against a blocking
  operation.

The `net` proposal (0023) anticipated this: "If Yar adds concurrency
primitives, the blocking socket model naturally extends to per-task blocking.
The opaque handle approach does not preclude this." The revised `time`
proposal (0024) keeps clock operations free of process-global timezone state
and defines blocking sleep as consuming only the calling native task thread.
Concurrency is the primary remaining capability gap.

### Why not `go` (fire-and-forget goroutines)?

Go's `go` keyword spawns a goroutine with no parent, no join, and no lifetime
bound. This violates Yar's "explicit over magical" and "control flow should
remain visible" principles. Goroutine leaks are Go's most common concurrency
bug. Yar should make the task lifetime syntactically visible.

### Why not `async`/`await`?

Async/await colors every function: async functions can only be called from async
contexts, or require special bridge functions. This creates two parallel APIs
across the entire ecosystem. Zig attempted language-level async and removed it in
0.14 due to compiler complexity and debugging problems. Rust's async model
requires `Pin`, executor choice, and splits the ecosystem between sync and async
libraries. Yar's "small surface area" principle rejects this complexity.

### Why not expose manual OS threads?

Zig's manual OS threads and synchronization are explicit but verbose and
low-level. Yar keeps task lifetime and join behavior in the `taskgroup`
surface. The current runtime uses native threads behind that structured API;
programs do not manage thread handles directly.

## 3. User-Facing Examples

### Valid examples

```
// Run two computations concurrently, collect both results
fn main() i32 {
    results := taskgroup []i32 {
        spawn compute(data_a)
        spawn compute(data_b)
    }
    print("sum: " + to_str(results[0] + results[1]) + "\n")
    return 0
}
```

```
// Producer-consumer with a channel
fn main() i32 {
    ch := chan_new[i32](16)
    taskgroup []void {
        spawn fn() void {
            for i := 0; i < 100; i += 1 {
                chan_send(ch, i) or |_| { break }
            }
            chan_close(ch)
        }()
        spawn fn() void {
            for {
                val := chan_recv(ch) or |_| { break }
                print(to_str(val) + "\n")
            }
        }()
    }
    return 0
}
```

```
// Parallel map: process slice elements concurrently
fn process_all(items []str) []!str {
    return taskgroup []!str {
        for i := 0; i < len(items); i += 1 {
            item := items[i]
            spawn process_item(item)
        }
    }
}
```

```
// Fan-out / fan-in with worker pool
fn main() i32 {
    jobs := chan_new[i32](64)
    results := chan_new[i32](64)

    taskgroup []void {
        // producer
        spawn fn() void {
            for i := 0; i < 1000; i += 1 {
                chan_send(jobs, i) or |_| { break }
            }
            chan_close(jobs)
        }()

        // 4 workers
        spawn fn() void {
            taskgroup []void {
                spawn worker(jobs, results)
                spawn worker(jobs, results)
                spawn worker(jobs, results)
                spawn worker(jobs, results)
            }
            chan_close(results)
        }()

        // consumer
        spawn fn() void {
            total := 0
            for {
                r := chan_recv(results) or |_| { break }
                total += r
            }
            print("total: " + to_str(total) + "\n")
        }()
    }
    return 0
}

fn worker(jobs chan[i32], results chan[i32]) void {
    for {
        job := chan_recv(jobs) or |_| { break }
        chan_send(results, job * job) or |_| { break }
    }
}
```

```
// Error handling: each task result is independently success or error
import "std/fs"

fn main() i32 {
    results := taskgroup []!str {
        spawn fs.read_file("a.txt")
        spawn fs.read_file("b.txt")
        spawn fs.read_file("c.txt")
    }
    for i := 0; i < len(results); i += 1 {
        content := results[i] or |err| {
            print("file " + to_str(i) + " failed: " + to_str(err) + "\n")
            continue
        }
        print(content)
    }
    return 0
}
```

### Invalid examples

```
// Cannot use spawn outside a taskgroup
spawn compute(data)  // error: spawn is only valid inside a taskgroup block
```

`spawn` is scoped to `taskgroup` blocks. There is no fire-and-forget spawning.

```
// Cannot ignore taskgroup result when tasks return values
taskgroup []i32 {
    spawn compute(1)
    spawn compute(2)
}
// error: taskgroup produces []i32 but result is unused
```

Taskgroup results must be consumed when tasks produce values, consistent with
Yar's existing rule that non-void expression statements are rejected.

```
// Cannot send wrong type through channel
ch := chan_new[i32](8)
chan_send(ch, "hello")  // error: expected i32, got str
```

Channels are typed. The type argument is explicit and enforced at compile time.

```
// Cannot use chan_send/chan_recv on non-channel values
x := 42
chan_send(x, 1)  // error: expected chan[T], got i32
```

Channel builtins require a `chan[T]` argument.

## 4. Semantics

### Taskgroup

A `taskgroup` is an expression that spawns concurrent tasks and blocks until all
complete.

- `taskgroup []R { ... }` evaluates the body, which may contain `spawn`
  expressions and ordinary statements including loops. The result type
  annotation `[]R` is required and specifies the element type of the returned
  slice.
- `spawn expr` starts one native thread immediately. The expression must be a
  named function call or an immediately called inline function literal whose
  return type matches `R`. Each `spawn` reserves one result slot in spawn order.
- `spawn` inside a `for` loop within a taskgroup is valid. Each iteration
  spawns an additional task.
- `spawn` inside nested `if` or `match` within a taskgroup is valid.
- The parent executes ordinary statements and `spawn` statements in source
  order. A spawned thread may overlap with later statements in the taskgroup
  body. The taskgroup joins all started threads after the body completes.
- The taskgroup expression evaluates to `[]R` containing one element per
  spawned task, in spawn order.
- If zero tasks are spawned (e.g., the loop body never executes), the result is
  an empty slice.
- Taskgroups may nest. An inner taskgroup blocks its enclosing task until all
  inner tasks complete. A `spawn` inside an inner taskgroup belongs to that
  inner taskgroup, not the outer one.
- Taskgroup with `[]void` result type: the expression evaluates to `[]void`
  but the caller typically uses it as a statement (the result is discardable
  for void slices, consistent with void expression statement rules).

### Task isolation

- Arguments are evaluated in the parent and copied into the task context using
  Yar's ordinary shallow value representation. Inline function literals also
  copy their captured environment; these copies are not deep copies.
- The checker therefore requires every argument and capture to be transitively
  share-safe. Scalars, `str`, and `error` are share-safe. Fixed arrays, enums,
  and non-resource structs are share-safe only when every contained value is
  share-safe. Typed `net.Conn` and `net.Listener` values are explicit
  share-safe resource references. `!T` and `chan[T]` are share-safe only when
  `T` is share-safe; `!void` is also share-safe.
- Pointers, slices, maps, interfaces, function values, file resource structs,
  and aggregates containing any of them cannot cross the task boundary.
- Channels are the synchronized mechanism for inter-task communication.
  Multiple tasks may hold the same share-safe channel and use its operations
  concurrently.
- The restriction applies to task inputs, not results. A taskgroup exposes
  results only after every spawned thread has joined.
- Bare `i64` values remain scalar. The checker cannot distinguish ordinary
  integers from runtime or OS handles represented as raw `i64` values.
- Runtime-backed raw handles are kind-checked, generation-tagged registry tokens
  whose mutable state is synchronized. Vacant slots may be reused with a new
  full token, while stale generations remain invalid. Stale-generation and
  wrong-kind access does not consume the current entry. That makes
  invalid-handle failure deterministic and serializes registered state access,
  but does not define concurrent operation order, provide compiler-visible
  handle provenance, or make raw handles valid share-safe inputs by design.

### Task scheduling

- Every successful `spawn` creates one native OS thread immediately.
- There is no task pool, work-stealing scheduler, parking, or M:N multiplexing
  in the current runtime.
- A blocking operation blocks that task's native thread. Other spawned threads
  continue independently.

### Task errors

- When `R` is `!T`, each task's result is independently a success or error
  value. No implicit cancellation occurs.
- When `R` is non-errorable (`i32`, `str`, etc.), a task that panics
  terminates the entire program, consistent with Yar's existing panic behavior.
- Error propagation across task boundaries uses the result slice: the caller
  inspects each `!T` element and handles errors with standard `or |err|`, `?`,
  or `return`.

### Channels

- `chan[T]` is a new built-in type representing a bounded, typed channel.
- `chan_new[T](capacity i32) chan[T]` creates a channel with the given buffer
  capacity. Capacity must be positive (at least 1). There are no unbounded or
  zero-capacity (rendezvous) channels.
- `chan_send(ch chan[T], value T) !void` sends a value into the channel. Blocks
  if the buffer is full. Returns `error.Closed` if the channel has been closed.
- `chan_recv(ch chan[T]) !T` receives a value from the channel. Blocks if the
  buffer is empty. Returns `error.Closed` if the channel is closed and the
  buffer is empty.
- `chan_close(ch chan[T]) void` closes the channel. After closing, sends return
  `error.Closed`. Receives drain remaining buffered values, then return
  `error.Closed`. Closing an already-closed channel is a no-op (not a panic).
- Channels are safe for concurrent use by multiple tasks. Multiple senders and
  multiple receivers are supported (MPMC).
- Channel ordering: values are received in FIFO order relative to sends.
- A channel value is a managed opaque token backed by synchronized Rust-owned
  state in a validated registry. Collection finalizes an unreachable token,
  removes that state, and stops treating its buffered values as roots. Live
  buffered slots remain collector roots while the channel token is reachable.

### Blocking behavior change

With concurrency, blocking calls (`net.accept`, `net.read`, `net.write`,
`fs.read_file`, and similar host calls) block the calling task's native thread,
not other spawned threads. The runtime does not park a task and reuse its
thread. A program with no taskgroups retains the single-threaded behavior.

### Thread safety of existing operations

- **Allocation lifetime**: the current runtime conservatively reclaims
  unreachable managed blocks. Collection is suppressed while spawned results
  remain unjoined. A taskgroup handle is consumed and reclaimed by its mandatory
  join; unreachable channel tokens finalize their external state.
- **Pointers, slices, and maps**: these values cannot be passed or captured
  across a spawn boundary, including when nested inside aggregates.
- **String builders**: `sb_new`/`sb_write`/`sb_string` resolve through the
  runtime handle registry and serialize mutable state per builder. Raw builder
  IDs remain indistinguishable from ordinary `i64` values to the checker.
- **Output**: `print` and `stdio.eprint` serialize each complete call under
  separate stdout and stderr locks. Calls have no ordering guarantee, but a
  single message is never torn by another runtime output call.
- **Fatal errors**: `panic` and runtime failures omit diagnostics whenever any
  task is unjoined, then terminate the whole process without waiting for output,
  shutdown handlers, or tasks. Single-threaded failures retain best-effort
  locked stderr diagnostics.
- **Process execution**: each call owns a separately contained child tree.
  Explicit deadlines, cancellation signals, and capture caps bound the call;
  blocking consumes only the calling native task thread. `process.args()`
  returns a snapshot and is safe.
- **`env.lookup`**: reads from the process environment through the Rust host
  runtime. Yar currently exposes no environment-mutation or timezone API.

## 5. Type Rules

### New type

- `chan[T]` is a built-in parameterized type. `T` may be any type except
  `void`, `noreturn`, first-class errorable `!U`, and `chan[U]` (no errorable
  elements or nested channels in v1).
- `chan[T]` supports `==` and `!=` comparison (identity comparison on the
  underlying handle).
- `chan[T]` does not support `<`, `>`, `<=`, `>=`, arithmetic, indexing,
  slicing, or field access.
- `chan[T]` is valid as a parameter type, return type, struct field type, slice
  element type, and closure capture type.
- `chan[T]` is not valid as a map key type.

### Taskgroup expression

- `taskgroup []R { body }` is an expression of type `[]R`.
- `R` must be a concrete type. `R` may be errorable (`!T`).
- `spawn expr` is valid only inside a `taskgroup` body (including inside
  loops, `if`, and `match` within the body). `spawn` is not valid inside a
  nested function literal within the taskgroup body — it must be at the
  taskgroup's own block level or inside control flow at that level.
- The spawned expression must be a named function call or an immediately
  called inline function literal whose return type is `R`. Arbitrary function
  values, builtins, and methods are rejected as spawn targets.
- Direct host intrinsics require a task wrapper. The current implementation has
  one for `fs.read_file`; other host calls must be wrapped in an inline literal.
- The spawned expression's arguments are evaluated in the enclosing scope at
  spawn time (sequentially, during the taskgroup body's execution).
- Spawn arguments and inline-literal captures must be transitively share-safe.
  Scalars, `str`, `error`, arrays, enums, non-resource structs, typed
  `net.Conn` and `net.Listener` references, errorable values, and channels
  compose only when their contained types are share-safe; `!void` is also
  share-safe. Pointers, slices, maps, interfaces, functions, and file resource
  structs are not share-safe.
- Spawn result types are not subject to this restriction because results are
  observed only after the taskgroup join.

### Channel builtins

- `chan_new[T](capacity i32) chan[T]` — `T` must be an explicit type argument.
  `capacity` must be `i32`.
- `chan_send(ch chan[T], value T) !void` — `T` is inferred from the channel
  argument. `value` must match `T`.
- `chan_recv(ch chan[T]) !T` — `T` is inferred from the channel argument.
- `chan_close(ch chan[T]) void` — `T` is inferred from the channel argument.

### Restrictions

- `spawn` outside a `taskgroup` body is a compile-time error.
- `taskgroup` in a non-function context (top level) is a compile-time error.
- Spawned expression must be a call — `spawn 42` or `spawn some_var` are
  compile-time errors.
- Spawning an arbitrary function value, builtin, or method is a compile-time
  error.
- Passing or capturing a non-share-safe value at a spawn boundary is a
  compile-time error.
- `chan_new` with capacity `<= 0` is a runtime error (trap).

## 6. Grammar / Parsing Shape

### Taskgroup

```
taskgroup_expr = "taskgroup" slice_type "{" taskgroup_body "}"
taskgroup_body = { statement | spawn_stmt }
spawn_stmt     = "spawn" call_expr
```

`taskgroup` is a new keyword. `spawn` is a new keyword valid only inside
`taskgroup` bodies. Neither conflicts with existing identifiers in user code
(if user code uses `taskgroup` or `spawn` as identifiers, this is a breaking
change that must be documented).

Precedence: `taskgroup` is a primary expression, like function literals. It
binds at expression level and the result can be assigned, returned, or used in
any expression position.

### Channel type

```
chan_type = "chan" "[" type "]"
```

`chan` is a new keyword. `chan[T]` is parsed as a type wherever types appear
(parameters, returns, fields, locals, etc.). The `[T]` uses the same bracket
syntax as generic type arguments.

### Ambiguity

No ambiguity with existing syntax:

- `taskgroup` starts with a keyword not currently in the grammar.
- `spawn` is only valid inside `taskgroup` and starts a statement.
- `chan[T]` as a type uses `chan` keyword followed by `[`, which is not a valid
  start for any existing type expression.
- `chan_new`, `chan_send`, `chan_recv`, `chan_close` are builtins following the
  existing naming pattern (`sb_new`, `sb_write`, `sb_string`).

## 7. Lowering / Implementation Model

### Parser impact

- New keywords: `taskgroup`, `spawn`, `chan`.
- New AST nodes: `TaskgroupExpr`, `SpawnStmt`, `ChanType`.
- `TaskgroupExpr` contains the result type annotation and the body block.
- `SpawnStmt` contains the call expression.
- `ChanType` contains the element type.

### AST / IR impact

- `TaskgroupExpr` node with fields: `ResultType`, `Body []Stmt`.
- `SpawnStmt` node with field: `Call Expr`.
- `ChanType` node with field: `ElemType Type`.
- `chan_new`, `chan_send`, `chan_recv`, `chan_close` are registered as builtins
  alongside `sb_new`, `sb_write`, `sb_string`.

### Checker impact

- Validate `taskgroup` result type is a valid slice type.
- Validate `spawn` is only inside a `taskgroup` body (not nested in a function
  literal within the body).
- Validate spawned expression is a call whose return type matches the taskgroup
  result element type.
- Validate that the target is a named function or immediately called inline
  function literal and that all arguments and captures are transitively
  share-safe.
- Register `chan[T]` as a parameterized built-in type.
- Register `chan_send`, `chan_recv`, `chan_close` with appropriate type
  inference from the channel argument.
- Register `chan_new` as a generic builtin requiring explicit type argument.
- Register `error.Closed` if not already registered (already exists from `net`
  package).

### Codegen impact

- **Taskgroup lowering**: the taskgroup body is emitted in source order. Each
  `spawn` creates its native thread immediately, and the end of the taskgroup
  joins all started threads before constructing the result slice.
- **Spawn lowering**: each `spawn call(args...)` is lowered to: (1) evaluate
  arguments, (2) copy the argument values into a task context, and (3) start a
  runtime task with a generated wrapper for the named function.
- **Closure spawns**: an immediately called function literal copies its closure
  value and arguments into the task context. Arbitrary closure values are not
  valid spawn targets.
- **Channel lowering**: `chan[T]` lowers to an opaque managed pointer token.
  The token addresses synchronized external state through a validated runtime
  registry and is finalized by collection. `chan_new` calls
  `yar_chan_new(elem_size, capacity)`. `chan_send` calls
  `yar_chan_send(handle, value_ptr)`. `chan_recv` calls
  `yar_chan_recv(handle, out_ptr)`. `chan_close` calls
  `yar_chan_close(handle)`.

### Original M:N runtime exploration

The runtime design below records the scheduler explored before implementation.
It is not the shipped runtime model; the current implementation creates one
native OS thread per spawn as described above and in section 14.

This is the largest implementation area — estimated ~2500-3500 lines of new C
and assembly code in the runtime.

**Context switching (~200 lines assembly + ~100 lines C):**

- Custom assembly for amd64 and arm64. Each switch saves callee-saved
  registers (6 on amd64 = 48 bytes, 20 on arm64 = 160 bytes), stores the
  stack pointer, loads the new stack pointer, restores registers, and returns.
  Approximately 15 instructions on amd64, 24 on arm64. Cost: ~10-30 ns per
  switch.
- Fallback to `setjmp`/`longjmp` for unsupported architectures (~30-100 ns).
- Files: `yar_context_amd64.S`, `yar_context_arm64.S`, `yar_context.c`
  (fallback + initialization).
- Each task has a `yar_task` struct: function pointer, argument data, stack
  base, stack size, saved context (register set), state enum (ready / running
  / blocked / done), result storage, cancellation flag, GC root registration.

**Task scheduler (~500 lines C):**

- Thread pool created at program start, one OS thread per core (configurable
  via `YAR_THREADS` environment variable, default: CPU count).
- Per-thread local run queue (fixed-size ring buffer, 256 slots) + global run
  queue (mutex-protected dequeue) + work-stealing from other threads' local
  queues.
- Scheduling loop per thread: (1) check local queue, (2) check global queue,
  (3) steal from random other thread's queue, (4) poll the netpoller for
  ready tasks, (5) park if no work.
- Preemption: not in v1. Tasks yield cooperatively at channel operations,
  I/O calls, and function calls that trigger stack growth checks. A sysmon-
  style background thread can be added later for preempting CPU-bound loops.

**Network I/O integration (~400 lines C):**

- Netpoller thread using epoll (Linux) / kqueue (macOS/BSD) / IOCP (Windows).
- All network fds are set to non-blocking mode (`O_NONBLOCK`).
- Modified `yar_net_*` functions: attempt the operation non-blocking. On
  `EAGAIN`/`EWOULDBLOCK`, register the fd with the poller, park the task,
  yield to the scheduler. When the poller signals readiness, re-queue the
  task.
- `time.sleep`: uses timerfd (Linux) / `EVFILT_TIMER` (macOS) / waitable
  timer (Windows) integrated with the poller. No thread consumed per sleeping
  task.
- File: `yar_netpoll_epoll.c`, `yar_netpoll_kqueue.c`, `yar_netpoll_iocp.c`,
  selected at compile time via `#ifdef`.

**File I/O and process thread pool (~200 lines C):**

- Pool of I/O threads (default: 4, configurable via `YAR_IO_THREADS`).
- Modified `yar_fs_*` and `yar_process_*` functions: park the calling task,
  submit the operation to an I/O thread, signal the scheduler when done.
- I/O threads are OS threads that block in kernel syscalls. They do not
  participate in the task scheduler — they only execute blocking operations
  and signal completion.

**Channel implementation (~300 lines C):**

- Bounded ring buffer with mutex + condition variable per channel.
- `yar_chan_new(elem_size, capacity)`: allocate channel struct via `yar_alloc`
  (GC-managed). Fields: buffer (elem_size × capacity bytes), head, tail,
  count, capacity, elem_size, closed flag, mutex, condvar, sender wait queue,
  receiver wait queue. Return opaque handle.
- `yar_chan_send(handle, value_ptr)`: lock, check closed (return error.Closed
  status if closed), if buffer full then add task to sender wait queue + park
  - yield (release lock, re-acquire on wake), `memcpy` value into buffer at
    tail, increment count and tail, signal one waiting receiver, unlock.
- `yar_chan_recv(handle, out_ptr)`: lock, if buffer empty and closed return
  error.Closed status, if buffer empty and not closed then add task to
  receiver wait queue + park + yield, `memcpy` value from buffer at head into
  out_ptr, decrement count and advance head, signal one waiting sender,
  unlock.
- `yar_chan_close(handle)`: lock, set closed flag, signal all waiting senders
  and receivers (they will re-check the closed flag and return error.Closed),
  unlock. Closing an already-closed channel is a no-op.
- Channel itself is GC-managed — no manual free. Unreferenced channels with
  buffered data are collected normally.

**GC modifications (~300 lines C):**

- Task stack registration: `yar_gc_register_stack(base, size)` adds a stack
  range to a dynamic array of root sets. `yar_gc_unregister_stack(base)`
  removes it.
- Stop-the-world barrier: a global `gc_stop_flag` checked by each scheduler
  thread at scheduling points (between task switches). When set, each thread
  saves the current task's context, acknowledges via an atomic counter, and
  waits on a condition variable. The GC thread waits for all acknowledgements
  before scanning.
- Stack scanning: iterates the task stack table, scanning each range
  [stack_current_sp, stack_base] conservatively (same algorithm as current
  single-stack scanning, applied N times).
- Parked tasks: their stack bounds are already recorded in the task struct.
  Running tasks: their registers are saved to the stack as part of the barrier
  acknowledgement, so the stack scan covers register contents.
- Channel buffers: no special handling — they are heap-allocated via
  `yar_alloc` and scanned as part of the normal heap traversal.
- Write barriers: not needed. The collector is STW — all mutators are stopped
  during collection. The graph is frozen and consistent.

**Race detector (~200 lines C):**

- Optional, enabled by `-race` flag on `yar build` / `yar run` / `yar test`.
- Uses LLVM's ThreadSanitizer instrumentation pass. When `-race` is active,
  the codegen emits `__tsan_readN`/`__tsan_writeN` calls around memory
  accesses and `__tsan_func_entry`/`__tsan_func_exit` at function boundaries.
- Task creation emits `__tsan_acquire`/`__tsan_release` to establish happens-
  before edges. Channel send/recv emit acquire/release pairs. Taskgroup join
  emits acquire.
- GC stop-the-world emits release (before scan) and acquire (after resume) to
  prevent false positives from GC scanning application memory.
- The TSan runtime library is linked when `-race` is active. Overhead: 5-10x
  memory, 2-20x CPU.
- Zero false positives in happens-before mode. Suitable for testing and CI.

## 8. Interactions

### Errors

Composes naturally. When `R` is `!T`, the taskgroup produces `[]!T`. Each
element is handled with standard `or |err|`, `?`, or `return`. No new error
handling mechanism is needed. `chan_send` returns `!void` and `chan_recv`
returns `!T`, using the existing `error.Closed` name from the `net` package.

### Structs

Non-resource struct values may cross a spawn boundary only when every field is
transitively share-safe. Resource structs and structs containing aliased values
are rejected.

### Enums

Enum values may cross a spawn boundary only when every case payload is
transitively share-safe.

### Slices

Slice descriptors are shallow aliases of backing storage, so slices cannot be
passed to or captured by spawned tasks.

### Maps

Map values are shared handles, so maps cannot be passed to or captured by
spawned tasks.

### Closures

Only an immediately called inline function literal may be a closure spawn
target. Its captures are shallow copies and must each be transitively
share-safe. Arbitrary closure values cannot be spawned.

### Control flow

`taskgroup` is an expression, not a statement. `break` and `continue` inside a
taskgroup body may affect loops within the body but cannot jump through an
enclosing loop outside it. `return` and same-function `?` propagation are
rejected in the taskgroup body so every accepted path reaches the join.

### New builtins

Four new builtins: `chan_new`, `chan_send`, `chan_recv`, `chan_close`. These
follow the naming and handle pattern established by `sb_new`, `sb_write`,
`sb_string`.

### Future: select

A future proposal may add a `select` construct for multiplexing across
multiple channel operations. This is deliberately excluded from v1 to keep the
surface area small. Most concurrent programs work with single-channel
producer/consumer patterns. If `select` becomes necessary, it can be added as
a new expression form without modifying the existing taskgroup or channel
semantics.

### Compile-time share-safety boundary

The checker rejects pointers and every other non-share-safe type in spawn
arguments and inline-literal captures. The rule is structural so an aggregate
cannot hide a mutable alias. Channels are allowed only when their element type
is share-safe, and task results remain unrestricted because they are observed
after join.

## 8a. Impact on Existing Builtins and Standard Library

This section records the original M:N runtime audit, not the shipped runtime
contract. Its proposed parking, netpoller, GC, and race-detector changes remain
unimplemented. The compile-time share-safety boundary above supersedes its
assumption that aliased values may freely cross spawn boundaries.

### Original runtime global-state assumptions

The original audit assumed a collector with six unsynchronized globals:

- `yar_gc_blocks` — linked list of all GC-managed allocations
- `yar_gc_bytes` — total allocated bytes
- `yar_gc_heap_target` — threshold for triggering collection
- `yar_gc_configured` — one-time init flag
- `yar_gc_collecting` — reentrancy guard
- `yar_gc_stack_top` — stack marker for root scanning

Those exact globals are not part of the shipped runtime. The Rust collector
uses synchronized external metadata, captures the main stack and preserved
registers, and defers collection rather than attempting to stop worker threads.

Additionally, `yar_env_lookup` and `yar_fs_temp_dir` call `getenv()`, which
is not thread-safe on POSIX (returns a pointer to shared global storage that
can be invalidated by concurrent `setenv`). This must be replaced with a
mutex-protected copy or platform-specific thread-safe alternatives.

### Builtins: impact by function

**Safe without changes (pure computation, no allocation, no global state):**

| Builtin      | Reason                                                         |
| ------------ | -------------------------------------------------------------- |
| `len`        | Reads length field from value. No allocation, no side effects. |
| `i32_to_i64` | Pure arithmetic conversion.                                    |
| `i64_to_i32` | Pure arithmetic conversion.                                    |

**Safe after GC mutex (allocate but no other shared state):**

| Builtin  | What it allocates                            | Change needed                                                                                                 |
| -------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `to_str` | Heap string via `yar_alloc`                  | GC mutex protects allocation.                                                                                 |
| `chr`    | Single-byte heap string via `yar_alloc`      | GC mutex protects allocation.                                                                                 |
| `append` | May reallocate slice backing via `yar_alloc` | GC mutex protects allocation. Concurrent append to the same slice is a data race (programmer responsibility). |
| `keys`   | Allocates snapshot slice via `yar_alloc`     | GC mutex protects allocation. Concurrent `keys` with map mutation is a data race.                             |

**Safe with synchronized or task-local state:**

| Builtin     | Shared handle risk                                                                              | Guidance                                                                                                           |
| ----------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `sb_new`    | Returns a new handle. Safe — each call creates an independent builder.                          | The registry allocates one typed process-local ID.                                                                 |
| `sb_write`  | Modifies builder state behind the validated handle registry.                                    | Per-builder synchronization serializes writes.                                                                     |
| `sb_string` | Reads and resets builder state behind the validated handle registry.                           | Per-builder synchronization serializes extraction and reset.                                                       |
| `has`       | Reads map structure without locking.                                                            | No runtime change. Document: maps must not be shared across tasks, or access must be serialized through a channel. |
| `delete`    | Modifies map, may trigger rehash.                                                               | Same as `has`.                                                                                                     |

**Require runtime changes:**

| Builtin | Issue                                                                                                                                                           | Required change                                                                                                                                                                                                                                                   |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `print` | Concurrent calls need call-level output atomicity. | Implemented: a stdout mutex covers each complete write. Calls may reorder, but messages are not torn. |
| `panic` | A fatal worker must not wait behind ordinary output or run shutdown handlers concurrently with other tasks. | Implemented: omit the diagnostic while tasks are unjoined, then immediately terminate the whole process without shutdown handlers. |

### Host intrinsics: `fs` package

| Function        | Safe?             | Change needed                                                                                                                                                                                                                              |
| --------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `fs.read_file`  | Yes               | Parks task, submits to I/O thread pool. No shared state.                                                                                                                                                                                   |
| `fs.write_file` | Yes (per-call)    | Parks task, submits to I/O thread pool. Concurrent writes to the same file are the programmer's responsibility (same as any OS-level file I/O).                                                                                            |
| `fs.read_dir`   | Yes               | Parks task, submits to I/O thread pool. Returns snapshot.                                                                                                                                                                                  |
| `fs.stat`       | Yes               | Parks task, submits to I/O thread pool. Idempotent.                                                                                                                                                                                        |
| `fs.mkdir_all`  | Yes (best-effort) | Parks task, submits to I/O thread pool. Internal stat-then-mkdir TOCTOU is harmless — `mkdir` returns `EEXIST` which the runtime already handles.                                                                                          |
| `fs.remove_all` | Yes (best-effort) | Parks task, submits to I/O thread pool. Concurrent removals of overlapping trees may see already-deleted entries; the runtime must tolerate `ENOENT` during recursive removal.                                                             |
| `fs.temp_dir`   | **No**            | Current implementation calls `getenv("TMPDIR")` which is not thread-safe. Fix: cache `TMPDIR` at program startup (before any tasks are spawned) in a global immutable string. The cached value is read-only, so concurrent access is safe. |

### Host intrinsics: `env` package

| Function     | Safe?  | Change needed                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------ | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `env.lookup` | **No** | `getenv()` is not thread-safe on POSIX. Fix: protect the `getenv` call and the subsequent string copy with a runtime mutex. The mutex must cover both `getenv()` and `memcpy()` of the returned pointer, because `getenv` returns a pointer into shared storage that can be invalidated by a concurrent `setenv`. On Windows, `GetEnvironmentVariableW` is thread-safe and does not need a mutex. |

### Host intrinsics: `process` package

| Function              | Safe?     | Change needed                                                                                                                                                                                                                                                                                               |
| --------------------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `process.args`        | Yes       | Reads `yar_host_argc`/`yar_host_argv` which are set once at startup and never modified. Safe for concurrent reads.                                                                                                                                                                                          |
| `process.run`         | Yes | Uses explicit timeout, cancellation, and independent capture caps. Containment cleanup completes before the blocked calling task resumes. |
| `process.run_inherit` | Yes | Uses explicit timeout and cancellation. Concurrent inherited children may interleave output because they write directly to inherited descriptors. |

### Host intrinsics: `stdio` package

| Function       | Safe? | Change needed                                                                                                                                        |
| -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `stdio.eprint` | Yes   | A stderr mutex covers each complete write and flush. |

### Historical host-intrinsic audit: `net` package

The table below is the original unimplemented M:N/netpoller design described by
the section preface. It is retained as historical design exploration and is not
the current networking contract.

| Function                                           | Thread-safe?   | Change needed                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| -------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `net.listen`                                       | Yes            | One-time init (`yar_net_ensure_init`) has a TOCTOU race on the static `initialized` flag. Fix: use `pthread_once` (POSIX) or `InitOnceExecuteOnce` (Windows). The listen call itself parks the task and submits to the netpoller.                                                                                                                                                                                                                         |
| `net.accept`                                       | Yes            | Parks task, registers listener fd with netpoller. When a connection arrives, the task is woken. Multiple tasks accepting on the same listener is safe — the OS serializes accept calls (thundering herd is a performance concern, not a correctness one).                                                                                                                                                                                                 |
| `net.connect`                                      | Yes            | Parks task, submits non-blocking connect to netpoller. Each connection is independent.                                                                                                                                                                                                                                                                                                                                                                    |
| `net.read`                                         | Per-connection | Parks task, registers fd with netpoller. If two tasks read the same connection concurrently, data is split arbitrarily between them. This is a programming error, not a runtime bug. Document: each connection should be owned by one task.                                                                                                                                                                                                               |
| `net.write`                                        | Per-connection | Parks task, submits non-blocking send. Same ownership rule as read.                                                                                                                                                                                                                                                                                                                                                                                       |
| `net.close` / `net.close_listener`                 | Per-connection | Closing a socket while another task is blocked on it is undefined behavior (the fd number could be reused for an unrelated file descriptor before the blocked task wakes). Fix: the netpoller must deregister the fd and wake any parked tasks with `error.Closed` before actually calling `close()`. This is how Go's netpoller handles closing — it sets a deadline of past time, which wakes all blocked goroutines with an error, then closes the fd. |
| `net.local_addr` / `net.remote_addr`               | Yes            | Read-only socket metadata. No blocking. No shared state.                                                                                                                                                                                                                                                                                                                                                                                                  |
| `net.listener_addr`                                | Yes            | Read-only socket metadata. No blocking.                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `net.set_read_deadline` / `net.set_write_deadline` | Per-connection | Sets socket option. If two tasks set different deadlines on the same connection concurrently, last write wins. This is a programming error. No runtime change needed — socket option setting is atomic at the syscall level.                                                                                                                                                                                                                              |
| `net.resolve`                                      | Yes            | `getaddrinfo` is thread-safe on all modern platforms. Parks task, submits to I/O thread pool (DNS can block).                                                                                                                                                                                                                                                                                                                                             |

The shipped native-thread contract instead uses typed, share-safe `Conn` and
`Listener` registry references. It permits one reader and one writer
concurrently, serializes same-direction operations, and makes close wake blocked
accept/read/write calls with `error.Closed` before waiting for cleanup. Raw
network IDs are internal. Read size is capped at 64 MiB inclusive; write is one
host write and may be short. Socket timeouts are relative per-operation limits,
and changing one need not interrupt an already-running syscall. Synchronous DNS
and connect cannot be interrupted before a handle exists.

### Host intrinsics: `time` package (proposed, not yet implemented)

The time package (proposal 0024) has not been implemented yet. The concurrency
contract is already shaped for the native-thread runtime:

| Function             | Concurrency contract |
| -------------------- | -------------------- |
| `time.now`           | Thread-safe wall-clock read with no global mutation. |
| `time.instant`       | Thread-safe monotonic read from one process origin. |
| `time.sleep`         | Blocks only the calling native task thread; sibling tasks continue. |
| UTC/text operations  | Deterministic pure package code with no timezone globals. |

Local and named timezone conversion is deferred from proposal 0024. A future
location design must use immutable explicit data and must not mutate `TZ`.
Timer-poller integration belongs only to the deferred M:N runtime exploration;
it is not required by the implemented native-thread task model.

### Stdlib pure packages: no changes needed

| Package | Functions                                              | Thread-safe?                                                                                     |
| ------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `path`  | `clean`, `join`, `dir`, `base`, `ext`                  | Yes — pure string processing, no allocation beyond string concatenation (protected by GC mutex). |
| `utf8`  | `decode`, `width`, `is_letter`, `is_digit`, `is_space` | Yes — pure computation, no allocation, no global state.                                          |
| `conv`  | `itoa`, `itoa64`, `to_i64`, `to_i32`, `byte_to_str`    | Yes — pure computation, allocations protected by GC mutex.                                       |

### Stdlib `strings` package

| Function                                                 | Thread-safe? | Notes                                                                                                                                                          |
| -------------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `contains`, `has_prefix`, `has_suffix`, `index`, `count` | Yes          | Pure string comparison, no allocation.                                                                                                                         |
| `trim_left`, `trim_right`, `trim`                        | Yes          | Returns substring via slicing, no allocation.                                                                                                                  |
| `parse_i64`                                              | Yes          | Pure parsing, no allocation.                                                                                                                                   |
| `repeat`, `replace`, `join`                              | Yes          | Allocate via string concatenation (GC mutex). Each call creates independent results.                                                                           |
| `to_lower`, `to_upper`                                   | Yes          | Each call creates a new string builder via `sb_new`, uses it privately, and extracts the result. The builder handle is local to the call — never shared. Safe. |
| `split`                                                  | Yes          | Allocates result slice via GC. Each call is independent.                                                                                                       |

### Stdlib `sort` package

| Function                  | Thread-safe? | Notes                                                                                                                                                                      |
| ------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `strings`, `i32s`, `i64s` | Per-slice    | In-place modification. Safe when each task sorts its own slice. Concurrent sorts of the same slice are a data race (programmer responsibility). No runtime changes needed. |

### Stdlib `testing` package

| Function                                                                 | Thread-safe? | Notes                                                                                                                                                                                                                                                    |
| ------------------------------------------------------------------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `new`, `equal`, `not_equal`, `is_true`, `is_false`, `fail`, `log`, `has_failed`, `message_count`, `message` | Per-T | Construct, modify, or read package-owned `*testing.T` state. Safe when each test has its own `T` instance (which is the case in the test runner). If `yar test` runs tests concurrently in the future, each test function receives a separate `T`. No runtime changes needed. |

### Summary of required runtime changes for existing code

**Critical (must be done before concurrency ships):**

1. **GC mutex**: protect `yar_alloc`, `yar_alloc_zeroed`, and
   `yar_gc_collect` with a mutex. Affects every function that allocates.
2. **`getenv` safety**: protect `yar_env_lookup` with a mutex that covers
   both the `getenv()` call and the copy of the returned string. Cache
   `TMPDIR` at startup for `yar_fs_temp_dir` and `yar_process_run`.
3. **stdout/stderr mutexes**: implemented; `yar_print` and `yar_eprint`
   serialize each complete call so individual messages are not torn.
4. **`panic` termination**: implemented; concurrent fatal paths skip output
   before immediate termination without shutdown handlers.
5. **Historical `net.close` safety requirement**: the proposed netpoller would
   deregister the fd and wake parked tasks before close. The shipped
   native-thread runtime meets the same user-visible wake-before-wait contract
   without a netpoller.
6. **`net.ensure_init` race**: replace static flag with `pthread_once`.
**Documentation-only (no runtime changes, behavior documented):**

7. Map handles shared across tasks are data races — document.
8. Raw string builder IDs bypass compile-time provenance checks — document.
9. Socket handles should be owned by one task — document.
10. Slice mutation across tasks is a data race — document.
11. `sort.*` on shared slices is a data race — document.
12. `fs.write_file` to the same path from multiple tasks races — document.
13. `process.run_inherit` from multiple tasks interleaves child output —
    document.

## 9. Alternatives Considered

### 1. Go-style goroutines (`go` keyword, fire-and-forget)

Would add a `go expr` statement that spawns a concurrent task with no lifetime
bound. Communication through channels, synchronization through `sync.WaitGroup`
or similar.

Rejected because:

- Fire-and-forget violates "control flow should remain visible."
- Goroutine leaks are Go's most common concurrency bug and are undetectable
  without external tooling.
- Requires separate `WaitGroup` or `errgroup` libraries to collect results —
  the language provides no structured join.
- Error propagation across goroutine boundaries requires manual channel plumbing
  for every error.

Taskgroups make lifetime visible, join automatic, and error collection
structural.

### 2. Rust-style async/await with futures

Would add `async fn` declarations that return `Future[T]`, `.await` syntax to
block on futures, and an executor runtime.

Rejected because:

- Colors every function — creates two parallel APIs in the ecosystem.
- Requires `Pin` or equivalent for self-referential state machines.
- Requires choosing an executor (runtime fragmentation).
- Enormous compiler complexity (Zig's experience confirms this).
- Violates "easy to explain" and "implementable without heroic machinery."

M:N scheduled tasks achieve the same I/O concurrency without function coloring.

### 3. Zig-style OS threads with manual synchronization

Would add `thread.spawn`, `thread.join`, `Mutex`, `RwLock`, `Condition`,
`Atomic` as stdlib types.

Rejected because:

- OS threads are expensive (~8MB stack), limiting concurrency to hundreds, not
  thousands.
- Manual mutex/lock programming is error-prone (deadlocks, forgotten locks).
- No structured lifetime — same leak problem as goroutines.
- Verbose: even "run N things and wait" requires manual thread management.
- Does not leverage Yar's existing GC runtime.

### 4. Actor model (Erlang/Elixir style)

Would make each concurrent entity a process with its own isolated heap,
communicating only through message passing. No shared memory.

Rejected because:

- Requires a per-actor heap and message serialization/copying — significant
  runtime complexity.
- The actor model is a full programming paradigm shift, not a minimal addition.
- Yar's GC is a shared heap collector; per-actor heaps would require a
  fundamentally different memory architecture.
- The taskgroup + channel model achieves actor-like isolation patterns (manager
  tasks that own state and respond to channel messages) without mandating the
  actor model for all concurrent code.

### 5. Channels with zero-capacity (rendezvous) and unbounded modes

Go channels support zero-capacity (synchronous rendezvous) and unbounded
(via `make(chan T)`) modes.

Rejected because:

- Zero-capacity channels add scheduling complexity (send and receive must
  happen simultaneously) and are a common source of deadlocks.
- Unbounded channels hide backpressure and can cause unbounded memory growth.
- Bounded-only channels are simpler to implement, reason about, and debug.
- If rendezvous semantics are needed, a capacity-1 channel approximates it.
  If truly unbounded buffering is needed, it can be built from a goroutine
  with an internal queue and two channels.

### 6. Implicit cancellation on first error

The taskgroup could automatically cancel remaining tasks when the first task
returns an error, similar to Go's `errgroup`.

Rejected because:

- Implicit cancellation is hidden control flow.
- Different use cases need different cancellation policies (fail-fast vs.
  collect-all vs. cancel-after-timeout).
- Explicit handling via the result slice lets the caller decide.
- A future `cancel` mechanism (via a cancellation channel or context) can be
  built on top of the existing primitives without changing them.

### 7. Mutex and shared state primitives

Add `Mutex[T]` (Rust-style, wrapping the protected data) as a primitive for
shared mutable state between tasks.

Deferred (not rejected) because:

- Channels + actor pattern cover most use cases.
- Mutex adds significant surface area (lock/unlock/try_lock, guard types or
  manual discipline).
- Rust's `Mutex[T]` approach (wrapping data) requires generic types with
  methods, which Yar does not support (methods on generic instantiations are
  not implemented).
- If real-world usage shows that the channel-based actor pattern is too
  cumbersome for certain patterns, a mutex proposal can follow.

## 10. Complexity Cost

- **Language and parser**: `taskgroup`, `spawn`, `chan[T]`, and four channel
  builtins add a bounded but cross-cutting surface.
- **Checker**: placement, join-safe control flow, target support, and
  transitive input share-safety all require explicit validation.
- **Codegen**: each spawn needs a typed task context and wrapper; taskgroups
  need ordered result assembly and a mandatory join.
- **Runtime**: the native-thread baseline is simpler than the explored M:N
  scheduler, but one thread per spawn, collection suppression while tasks are
  unjoined, and process-lifetime string-builder handles remain material
  operational limits. Taskgroups and unreachable channel state are reclaimed.
- **Testing**: concurrency ordering, channel closure, nested captures, resource
  provenance, and task-boundary alias rejection need end-to-end coverage.

## 11. Why This Direction?

The structured-concurrency surface is implemented. The original exploration
served to:

1. **Establish the design direction** before implementation begins, so that
   decisions about the `time` package (0024), future stdlib additions, and
   runtime architecture can account for concurrency requirements.
2. **Identify the blast radius** — specifically, which existing components
   (GC, host intrinsics, blocking operations, and output) need modification.
3. **Commit to structured concurrency** as the model, ruling out
   fire-and-forget goroutines and async/await early, so library design and
   user expectations align.
4. **Keep the first surface bounded** — select, mutexes, and a race detector
   remain outside it, while a conservative compile-time share-safety rule
   closes the task-input aliasing boundary.

The time proposal must preserve this concurrency model: clock operations avoid
global mutation, and blocking sleep consumes only the calling native task
thread. Local and named timezone work remains separate and deferred.

## 12. Resolved Design Decisions

Each decision was informed by surveying Go, Rust, Zig, Kotlin, Swift, Java,
and Python implementations, academic research, and production deployment data.

### Context switching: custom assembly per architecture

**Decision**: use hand-written assembly for context switching on each supported
architecture (amd64, arm64), with a `setjmp`/`longjmp` fallback for
unsupported targets.

**Research findings**:

- `makecontext`/`swapcontext` call `sigprocmask` (a syscall) on every switch,
  costing ~200-500 ns. They are deprecated in POSIX.1-2008 and incompatible
  with AddressSanitizer.
- `setjmp`/`longjmp` avoid the syscall but cost ~30-100 ns due to saving more
  state than necessary. Some platforms mangle stack pointers in `jmp_buf` for
  security, breaking direct manipulation. Do not support stack growth.
- Custom assembly costs ~10-30 ns per switch. Go uses this approach exclusively
  (hand-written `gosave`/`gogo`/`mcall` in `runtime/asm_<arch>.s`), saving
  only callee-saved registers (6 on amd64, 20 on arm64) and the stack/program
  counter — approximately 15 instructions on amd64, 24 on arm64.
- Windows fibers cost ~8-30 ns but have TLS problems when tasks migrate between
  OS threads (fiber-local storage exists but standard `thread_local` breaks).
- Production coroutine libraries (libaco, libco, Tencent libco) all use custom
  assembly for their fast paths.

**Context sizes**: 48 bytes on amd64 (6 registers × 8), 160 bytes on arm64
(20 registers × 8). Negligible compared to task stack sizes.

**Fallback**: `setjmp`/`longjmp` provides portability to any POSIX target at
the cost of ~3-10x slower switching. Acceptable because context switching is
not the bottleneck for I/O-bound programs.

### Blocking I/O: hybrid model (netpoller + I/O thread pool)

**Decision**: use epoll (Linux) / kqueue (macOS/BSD) for network I/O, and a
dedicated I/O thread pool for file I/O and child process waiting.

**Research findings**:

- **Network I/O through netpoller**: Go's approach. Set network fds to
  non-blocking mode. When a task calls `net.read` and gets `EAGAIN`, park the
  task and register interest with the poller. When the fd becomes ready, wake
  the task. This allows 100K+ concurrent connections with minimal threads.
- **File I/O cannot use epoll**: on Linux, `epoll_ctl` returns `EPERM` for
  regular file fds. Regular files are always "ready" from the kernel's
  perspective — they block in the page cache. Go uses a thread-per-blocked-
  syscall model for file I/O; tokio uses a dedicated blocking thread pool
  (default 512 threads). Both approaches work.
- **Child process waiting**: on Linux 5.3+, `pidfd_open` + epoll integration
  is the clean solution. On macOS, kqueue's `EVFILT_PROC` with `NOTE_EXIT`
  handles this natively. Fallback: `SIGCHLD` + signalfd or the self-pipe
  trick.
- **io_uring**: handles both file and network I/O but has a significant
  history of security vulnerabilities (Google found 60% of kernel exploits in
  2022 targeted io_uring). Requires Linux 5.6+ for full features. Not
  suitable as the sole I/O mechanism for v1 but can be added as an optional
  backend later.

**Design**:

- Network I/O: a single poller thread per runtime uses epoll/kqueue. Tasks
  that block on network calls are parked and added to the poller. When
  readiness fires, the task is re-queued. Zero OS threads consumed per waiting
  connection.
- File I/O: a pool of I/O threads (default: 4, configurable via
  `YAR_IO_THREADS`). When a task calls `fs.read_file` or similar, the task is
  parked, the operation is submitted to an I/O thread, and on completion the
  task is re-queued. This follows tokio's `spawn_blocking` model.
- Process waiting: on Linux 5.3+, use pidfd + epoll. On macOS, use
  `EVFILT_PROC`. Fallback: submit `waitpid` to the I/O thread pool.
- `time.sleep`: use timerfd (Linux) or `EVFILT_TIMER` (macOS) integrated with
  the poller. The task is parked and woken when the timer fires. No thread
  consumed per sleeping task.

### Task stack size: 4 KB initial, growable with software checks

**Decision**: 4 KB initial stack per task, doubling on overflow up to 8 MB
maximum, with compiler-inserted stack bound checks in function prologues.

**Research findings**:

- Go uses 2 KB initial stacks with contiguous stack copying on growth
  (doubling). This requires precise pointer maps so the runtime can adjust
  pointers when copying. Yar's conservative GC cannot safely copy stacks
  (conservative scanning cannot distinguish pointers from integers, so pointer
  adjustment is impossible).
- Since Yar cannot copy stacks, growth must use a **segmented model** or
  **guard page** approach. Segmented stacks had a "hot split" problem in Go
  1.0-1.2 where tight loops near segment boundaries repeatedly alloc/free.
  Guard pages have zero per-call overhead but require 4 KB of virtual address
  space per task and cannot support fine-grained growth.
- **Chosen approach**: guard pages for overflow detection + runtime stack
  reallocation. Each task stack is allocated with a guard page at the bottom.
  When a function prologue's stack frame would exceed remaining space, the
  runtime allocates a larger stack, copies the contents, and updates the task's
  stack bounds. Unlike Go, Yar does not adjust interior pointers — instead,
  pointers to stack-allocated values in Yar already trigger heap promotion
  (conservative implementation allocates addressed locals on the heap). This
  means stack copying is safe: the stack contains no interior pointers to
  itself.
- 4 KB is chosen over 2 KB because: (a) most tasks in Go never grow beyond
  4 KB, (b) Yar's stack growth is more expensive than Go's (no precise
  pointer maps), so reducing growth frequency matters, (c) 4 KB still allows
  ~262,000 tasks per GB of stack memory alone.
- 8 MB maximum matches Rust's default thread stack and prevents runaway
  recursion from consuming all memory.

**Density**: at 4 KB per task, ~262K tasks per GB. At average 8 KB (after some
growth), ~131K tasks per GB. Sufficient for all but the most extreme
connection-per-task servers.

### Taskgroup body semantics: streaming spawn (immediate start)

**Decision**: tasks start executing immediately when `spawn` is evaluated, not
after the taskgroup body completes.

**Research findings**:

All production structured concurrency implementations use streaming (immediate)
spawn:

- Swift's `group.addTask` starts the task immediately.
- Java's `scope.fork()` starts a virtual thread immediately.
- Kotlin's `launch`/`async` start executing immediately.
- Python Trio's `nursery.start_soon()` schedules for immediate execution.

No production implementation uses "bulk spawn" (collect all, then start). The
reason: streaming spawn is more flexible (supports dynamic task counts from
loops, conditional spawning, accept-loop patterns like TCP servers) and the
scoping guarantees already ensure all tasks complete before the scope exits.

**Implication for control flow**: `return` and same-function `?` propagation
are rejected inside a taskgroup body. Accepted paths therefore reach the join;
the current runtime has no task cancellation semantics.

### Channel capacity: minimum 1, no rendezvous, no maximum

**Decision**: minimum capacity is 1 (no zero-capacity rendezvous channels).
No enforced maximum capacity — the programmer chooses the size.

**Research findings**:

- Go's unbuffered channels (capacity 0) are synchronous rendezvous: send
  blocks until a receiver is ready, and vice versa. They are a common source
  of deadlocks, especially for beginners. Go's internal channel implementation
  uses a separate code path for unbuffered channels, adding complexity.
- Real-world Go channel usage: most production channels are buffered with
  small capacities (1-100). Unbuffered channels are used primarily for
  signaling, not data transfer.
- Capacity-1 channels approximate rendezvous semantics with one message of
  slack. The semantic difference matters only for strict synchronization
  protocols, which are rare in application code.
- No maximum is needed: the programmer explicitly chooses the capacity and is
  responsible for memory. Very large capacities are a deliberate choice (e.g.,
  64K-element work queues). An OOM from a large channel is the same class of
  error as an OOM from a large slice allocation.

### Task input sharing: compile-time restriction

**Decision**: reject task inputs that can carry aliased mutable state. The
checker evaluates spawn argument and inline-literal capture types transitively.
Pointers, slices, maps, interfaces, functions, resource structs, and aggregates
containing them are rejected. Channels remain the explicit synchronized
sharing mechanism, and `chan[T]` is accepted only when `T` is share-safe.

This boundary favors a small static rule over a borrow checker or a runtime
race detector. It is intentionally conservative: read-only pointer sharing and
disjoint slice partitioning are not expressible across a spawn boundary. Task
results are unaffected because taskgroup join establishes exclusive parent
observation. Bare `i64` capabilities remain a limitation because their handle
provenance is not represented in the type. The runtime validates registered IDs
and resource kinds and synchronizes their state, but that does not make the
source type nominally safe.

### Nested spawn: disallowed in closures within taskgroup body

**Decision**: `spawn` is only valid at the taskgroup's own block level or
inside control flow (`for`, `if`, `match`) at that level. `spawn` inside a
function literal within the taskgroup body is a compile-time error.

**Rationale**: Allowing `spawn` inside a closure would create an implicit
reference from the closure to the enclosing taskgroup. This violates "explicit
over magical" — the closure could be stored, passed to another function, or
called after the taskgroup exits. Rejecting this at compile time is simple and
prevents a class of lifetime bugs. If a closure needs to spawn, it can contain
its own inner `taskgroup`.

### Timezone work is deferred from the time proposal

**Decision**: proposal 0024 contains UTC operations only. Local and named
timezone conversion requires a separate proposal defining immutable location
values, timezone-data ownership and versioning, Windows/IANA mapping, and DST
gap/fold behavior. No future design may implement conversion by mutating
process-global `TZ` state.

### Child process waiting in the deferred M:N design

The implemented native-thread runtime blocks only the calling task thread and
uses platform process-group or Job Object containment. A future M:N runtime
would move the same bounded operation to an I/O thread pool so its worker
thread can run other tasks.

**Research findings**:

- On Linux 5.3+, `pidfd_open` + epoll integration is clean (the pidfd becomes
  readable when the child exits). On macOS, kqueue's `EVFILT_PROC` with
  `NOTE_EXIT` handles this natively.
- However, `process.run` in Yar also captures stdout/stderr, which requires
  reading from pipes. Pipe reads can be integrated with the netpoller (pipes
  are pollable on both Linux and macOS).
- The simplest M:N approach: submit the entire `process.run`
  operation (fork + exec + pipe reads + waitpid) to an I/O thread. The task
  is parked and resumed when the I/O thread signals completion. This avoids
  splitting the operation across poller and thread pool.
- Future optimization: split pipe reads into the netpoller and only waitpid
  into the thread pool (or use pidfd/EVFILT_PROC).

### Cross-compilation

**Decision**: the runtime includes platform-specific context switching and I/O
multiplexing code, selected at compile time via `#ifdef` (same pattern as
existing `#ifdef _WIN32` in the runtime).

**Required platform code**:

| Component         | Linux                                   | macOS/BSD                                          | Windows                           |
| ----------------- | --------------------------------------- | -------------------------------------------------- | --------------------------------- |
| Context switch    | Custom asm (amd64, arm64)               | Custom asm (amd64, arm64)                          | Custom asm (amd64, arm64)         |
| Network poller    | epoll                                   | kqueue                                             | IOCP                              |
| Timer integration | timerfd + epoll                         | EVFILT_TIMER                                       | CreateWaitableTimer + IOCP        |
| Process waiting   | pidfd + epoll (5.3+), fallback signalfd | EVFILT_PROC                                        | WaitForSingleObject on I/O thread |
| Thread creation   | pthreads                                | pthreads                                           | CreateThread                      |
| Mutex/condvar     | pthreads                                | pthreads                                           | SRW locks + condition variables   |

Assembly files: `yar_context_amd64.S` and `yar_context_arm64.S`, included
conditionally based on `YAR_ARCH`. The `setjmp`/`longjmp` fallback covers
any architecture without dedicated assembly.

### GC modifications for multi-task scanning

This subsection records the deferred M:N scheduler design, not shipped runtime
behavior. The current native-thread baseline defers collection while any
spawned result is unjoined; it does not pause or scan worker threads.

**Deferred M:N decision**: follow the Boehm GC multi-thread model —
stop-the-world across all OS threads, scan all task stacks, then resume.

**Research findings**:

- A future M:N runtime can retain conservative mark-and-sweep without making
  collection concurrent. Its critical change would be stopping mutators and
  scanning multiple registered stacks instead of deferring collection.
- Boehm GC handles multiple threads by: (a) registering each thread with the
  collector via `GC_register_my_thread`, (b) stopping all threads with
  `SIGUSR1` (or `SuspendThread` on Windows) during collection, (c) scanning
  each thread's stack between its current SP and its recorded stack base.
- For Yar: each task stack is registered with the GC when created and
  unregistered when destroyed. During STW, all OS threads in the thread pool
  are paused (via a global barrier — each thread checks a flag at scheduling
  points). All task stacks (both running and parked) are scanned. Parked tasks
  have their stack bounds already recorded. Running tasks have their registers
  saved to the stack before the barrier.
- No write barriers needed — Yar's STW collector sees a frozen snapshot of
  all memory during collection. Write barriers would only be needed for
  concurrent or incremental marking, which is not in scope.
- The current native-thread runtime keeps channel buffers in Rust-owned storage
  and snapshots live slots as explicit roots. A future M:N runtime may instead
  move them into managed heap blocks, but that is not current behavior.

## 13. Remaining Open Questions

### Stack growth without pointer maps

Yar's conservative GC cannot distinguish pointers from integers on the stack.
This means stack copying (Go's approach) requires copying without adjusting
interior pointers. The proposal asserts this is safe because Yar heap-promotes
addressed locals. This claim needs verification: are there any cases where the
stack contains self-referential pointers (e.g., frame pointers, return address
chains) that would break after copying? The compiler's frame layout must be
audited.

### GC stop-the-world latency at scale

With many tasks, scanning all stacks during STW may cause noticeable pauses.
Go reduced this with precise stack maps (O(pointer_slots) per stack instead of
O(stack_bytes)). Yar's conservative scanner is O(stack_bytes). At 10K tasks
with average 4 KB stacks, the scanner must examine ~40 MB of stack data. Is
this acceptable? Benchmarking is needed to determine if incremental or
concurrent marking should be a future goal.

### LLVM TSan integration with conservative GC

TSan may report false positives when the GC scanner reads application memory
during STW. The GC's stop-the-world synchronization must be annotated with
`__tsan_acquire`/`__tsan_release` to establish happens-before edges. This
interaction is under-documented for conservative collectors specifically. A
prototype is needed to validate that annotations are sufficient.

### Windows IOCP integration

IOCP uses a completion-based model (submit I/O with a buffer, get notified
when done) unlike epoll/kqueue's readiness-based model (get notified when fd
is ready, then do I/O). This requires different codegen patterns for I/O
operations on Windows. The abstraction layer must handle both models behind a
common task-parking interface. This adds significant platform-specific runtime
code.

## 14. Decision

Accepted as Yar's structured-concurrency design. The delivery state records the
implemented baseline.
The shipped implementation includes:

- `taskgroup []R { ... }`
- `spawn call(...)` inside taskgroups
- `chan[T]` plus `chan_new`, `chan_send`, `chan_recv`, and `chan_close`
- `taskgroup []!T` and `taskgroup []void`
- immediate task start at each `spawn`
- named-function and immediate-inline-literal spawn targets
- transitive share-safety checking for task arguments and captures

The current runtime implementation deliberately differs from the original
exploration in a few ways:

- it uses native OS threads rather than an M:N scheduler
- each spawn receives shallow value copies, so pointers, slices, maps,
  interfaces, functions, resource structs, and aggregates containing them are
  rejected at the task boundary; channels compose only with share-safe element
  types
- task results are unrestricted because they are exposed after join
- bare raw handles represented as `i64` are not distinguishable from ordinary
  integers by this check; the runtime validates their registry ID and resource
  kind and synchronizes registered state
- it currently rejects `return` in a taskgroup body and rejects `break` /
  `continue` that would exit the taskgroup through an enclosing loop
- it rejects same-function `?` propagation inside a taskgroup body so accepted
  control-flow paths cannot bypass the join
- it does not yet include a race-detector mode
- Linux, macOS, and Windows GNU use the portable native-thread runtime; CI
  executes taskgroup, channel, and forced-collection fixtures on Windows

## 15. Implementation Checklist

This section tracks the implementation status against the original plan.

### Phase 1: Deferred M:N runtime exploration

These items belong to the unshipped M:N design, not the accepted native-thread
baseline:

- [ ] `crates/yar-runtime/src/` — amd64 context switch assembly
- [ ] `crates/yar-runtime/src/` — arm64 context switch assembly
- [ ] `crates/yar-runtime/src/` — context init, setjmp/longjmp fallback
- [ ] `crates/yar-runtime/src/` — thread pool, run queues, work
      stealing, task lifecycle
- [ ] `crates/yar-runtime/src/` — Linux network poller
- [ ] `crates/yar-runtime/src/` — macOS/BSD network poller
- [ ] `crates/yar-runtime/src/` — Windows network poller
- [ ] `crates/yar-runtime/src/` — blocking I/O thread pool
- [ ] `crates/yar-runtime/src/memory.rs` — GC multi-stack scanning,
      stop-the-world barrier, task stack registration
- [ ] `crates/yar-runtime/src/net.rs` — modify yar*net*\* for non-blocking
      mode with poller integration
- [ ] `crates/yar-runtime/src/filesystem.rs` and
      `crates/yar-runtime/src/host.rs` — modify yar*fs*_, yar*process*_ for
      I/O thread pool submission

The shipped baseline instead has:

- [x] `crates/yar-runtime/src/concurrency.rs` — native-thread taskgroups and
      bounded channels

### Phase 2: Language surface

- [x] `crates/yar-compiler/src/token.rs` — new keywords: `taskgroup`, `spawn`,
      `chan`
- [x] `crates/yar-compiler/src/ast.rs` — new AST nodes: `TaskgroupExpr`, `SpawnStmt`,
      `ChanType`
- [x] `crates/yar-compiler/src/parser.rs` — parse taskgroup, spawn, chan type
- [x] `crates/yar-compiler/src/checker.rs` — taskgroup type rules, spawn validation,
      channel builtin registration, `chan[T]` type
- [x] `crates/yar-compiler/src/codegen.rs` — taskgroup lowering, channel handle
      codegen, task-context construction, and task wrapper generation

### Phase 3: Deferred race detector

- [ ] `crates/yar-compiler/src/codegen.rs` — conditional TSan instrumentation when
      `-race` flag is active
- [ ] `crates/yar-compiler/src/compile.rs` — `-race` flag handling, TSan runtime
      library linking
- [ ] `crates/yar-runtime/src/` — TSan annotation helpers for GC barriers,
      channel operations, task creation/join

### Phase 4: Tests and documentation

- [x] `testdata/concurrency_basic/main.yar` — basic taskgroup test
- [x] `testdata/concurrency_channels/main.yar` — channel test
- [x] `testdata/concurrency_errors/main.yar` — error propagation test
- [x] `testdata/concurrency_lifecycle/main.yar` — repeated taskgroup/channel
      reclamation under forced collection
- [x] `testdata/concurrency_share_safe/main.yar` — transitive share-safety and
      unrestricted result test
- [x] `testdata/stdlib_net/main.yar` — concurrent typed TCP server/client test
- [ ] `testdata/concurrency_race/main.yar` — race detector validation
- [x] Rust compiler and CLI tests — concurrency test functions
- [x] `docs/context/domains/concurrency.md` — concurrency documentation
- [x] `docs/context/domains/language-slice.md` — updated with taskgroup, spawn,
      chan
- [x] `docs/context/domains/stdlib.md` — channel builtins
- [x] `docs/context/platform/toolchain-runtime.md` — current native-thread and
      channel runtime boundaries
- [x] `docs/context/summary.md` — updated capabilities
- [x] `docs/YAR.md` — concurrency reference
- [x] `LLM.txt` — concurrency reference
- [x] `README.md` — updated feature list
