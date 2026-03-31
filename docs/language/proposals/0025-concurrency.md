# Proposal: Structured Concurrency (`task` and `chan`)

Status: exploring

## 1. Summary

Add structured concurrency to Yar through two constructs: `taskgroup` blocks
that spawn and join concurrent tasks with guaranteed lifetime, and typed bounded
channels for inter-task communication. Tasks are M:N scheduled onto a runtime
thread pool. The design leverages Yar's existing value-capture closures for
natural task isolation, composes with `!T` error returns for explicit error
handling across task boundaries, and introduces no function coloring. All
blocking operations (I/O, channel, sleep) block the calling task, not the
program.

## 2. Motivation

Yar is currently single-threaded. All blocking calls (networking, sleep, file
I/O) halt the entire program. This prevents:

- **Concurrent servers**: a TCP server cannot handle more than one connection
  at a time because `net.accept` and `net.read` block the program.
- **Parallel computation**: CPU-bound work cannot use multiple cores.
- **Background work with I/O overlap**: a program cannot download a file while
  processing another.
- **Timed operations**: there is no way to race a timeout against a blocking
  operation.

The `net` proposal (0023) anticipated this: "If Yar adds concurrency
primitives, the blocking socket model naturally extends to per-task blocking.
The opaque handle approach does not preclude this." The `time` proposal (0024)
notes `sleep` blocks the calling program and that `date_in` uses non-thread-safe
`setenv`/`tzset` because "Yar is single-threaded." Concurrency is the primary
remaining capability gap.

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

### Why not OS threads only?

Zig's current approach (manual OS threads + synchronization) is explicit but
verbose and low-level. OS threads are expensive (~8MB stack each), limiting
concurrent connection counts. Yar's GC already requires a runtime — adding M:N
scheduling to that runtime is a natural extension, not a new dependency.

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
// Concurrent server: accept connections and handle each in a task
import "net"

fn main() !i32 {
    ln := net.listen("0.0.0.0", 8080)?
    taskgroup []!void {
        // acceptor task
        spawn fn() !void {
            for {
                conn := net.accept(ln)?
                // each connection handled concurrently
                spawn fn() !void {
                    handle_connection(conn)
                }
            }
        }
    }
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
        }
        spawn fn() void {
            for {
                val := chan_recv(ch) or |_| { break }
                print(to_str(val) + "\n")
            }
        }
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
        }

        // 4 workers
        spawn fn() void {
            taskgroup []void {
                spawn worker(jobs, results)
                spawn worker(jobs, results)
                spawn worker(jobs, results)
                spawn worker(jobs, results)
            }
            chan_close(results)
        }

        // consumer
        spawn fn() void {
            total := 0
            for {
                r := chan_recv(results) or |_| { break }
                total += r
            }
            print("total: " + to_str(total) + "\n")
        }
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
import "fs"

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
- `spawn expr` queues a function call for concurrent execution. The expression
  must be a function call whose return type matches `R`. Each `spawn` appends
  one element to the result slice when its task completes.
- `spawn` inside a `for` loop within a taskgroup is valid. Each iteration
  spawns an additional task.
- `spawn` inside nested `if` or `match` within a taskgroup is valid.
- Ordinary statements (variable declarations, assignments, function calls) in
  the taskgroup body execute sequentially before any spawned tasks begin.
  `spawn` statements are collected during the body's sequential execution and
  all spawned tasks begin executing concurrently after the body completes. This
  is the "bulk spawn" model: the body is a sequential setup phase that
  registers tasks, followed by concurrent execution and join.
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

- A spawned expression is a function call. Arguments are evaluated in the
  parent scope and passed by value to the task, following Yar's existing
  calling convention.
- When the spawned expression is an anonymous function literal (closure), outer
  variables are captured by value and are read-only in the closure body. This
  is Yar's existing closure semantics — no new rules.
- There is no shared mutable state between tasks through the language's value
  passing or closure capture mechanisms. Two tasks that both capture `x` each
  get independent copies.
- Channels are the intended mechanism for inter-task communication. Channel
  values are opaque handles (like map and string builder handles) and can be
  captured by closures or passed as arguments. Multiple tasks holding the same
  channel handle can send and receive concurrently.
- Pointers: if a task receives a pointer (through an argument or capture), and
  another task holds a pointer to the same object, both tasks can read and
  write the shared object concurrently. **This is a data race and the behavior
  is undefined.** The language does not prevent pointer sharing across tasks in
  v1. The runtime race detector (section 7) catches this at runtime. A future
  proposal may add compile-time restrictions on pointer sharing across task
  boundaries.

### Task scheduling

- The runtime maintains a pool of OS threads, defaulting to the number of CPU
  cores.
- Tasks are multiplexed onto OS threads by a work-stealing scheduler.
- When a task blocks (channel operation, network I/O, sleep, file I/O), the
  runtime parks the task and runs another task on the same OS thread.
- When a blocked task's I/O or channel operation completes, the runtime
  re-queues it for execution.
- Task stacks start small (configurable, default 8KB) and grow as needed,
  managed by the runtime.

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
- Channels are garbage-collected like other heap-backed values. A channel with
  no remaining references is collected even if it has buffered values.

### Blocking behavior change

With concurrency, blocking calls (`net.accept`, `net.read`, `net.write`,
`fs.read_file`, `time.sleep`, etc.) block the calling **task**, not the
program. Other tasks continue executing on other OS threads or on the same OS
thread after the blocked task is parked. This is a behavioral change from the
single-threaded model but is backwards compatible: a program with no taskgroups
has exactly one task (the main task) and behaves identically to the
single-threaded model.

### Thread safety of existing operations

- **GC**: the garbage collector must support multiple task stacks as root sets
  and coordinate stop-the-world pauses across all OS threads. This is an
  implementation change to the runtime, not a language change.
- **Slices**: concurrent reads of the same slice from multiple tasks are safe.
  Concurrent writes to the same slice, or a write concurrent with a read, are
  data races (undefined behavior). The runtime race detector catches this.
- **Maps**: concurrent access to the same map from multiple tasks is a data
  race (undefined behavior). Maps should be accessed from a single task or
  protected by a channel-based serialization pattern.
- **String builders**: `sb_new`/`sb_write`/`sb_string` are not safe for
  concurrent use. Each builder should be used by one task.
- **`time.date_in` / `time.from_date_in`**: these functions manipulate the
  process-global `TZ` environment variable. With concurrency, this is no
  longer safe. These functions must be changed to use thread-local timezone
  conversion or a mutex. This is an implementation fix in the runtime, not a
  language change.
- **`print`**: `print` writes to stdout. Concurrent `print` calls from
  multiple tasks may interleave output. Each individual `print` call is atomic
  (the full string is written in one `write` syscall), but ordering between
  tasks is not guaranteed.
- **`process.run`**: safe to call concurrently (each call creates a separate
  child process). `process.args()` returns a snapshot and is safe.
- **`env.lookup`**: reads from the process environment. Safe for concurrent
  reads but unsafe if any task modifies the environment via
  `time.from_date_in` or similar. The runtime mutex for timezone operations
  must also protect environment reads.

## 5. Type Rules

### New type

- `chan[T]` is a built-in parameterized type. `T` may be any type except
  `void`, `noreturn`, and `chan[U]` (no nested channels in v1).
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
- The spawned expression must be a function call (named function, qualified
  function, method call, or function-value call) whose return type is `R`.
- The spawned expression's arguments are evaluated in the enclosing scope at
  spawn time (sequentially, during the taskgroup body's execution).

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
- Register `chan[T]` as a parameterized built-in type.
- Register `chan_send`, `chan_recv`, `chan_close` with appropriate type
  inference from the channel argument.
- Register `chan_new` as a generic builtin requiring explicit type argument.
- Register `error.Closed` if not already registered (already exists from `net`
  package).

### Codegen impact

- **Taskgroup lowering**: the taskgroup body is emitted as sequential code that
  collects spawn descriptors (function pointer + argument struct) into a
  runtime array. After the body, a runtime call `yar_taskgroup_run(spawns,
count, result_ptr)` executes all tasks concurrently, blocks until all
  complete, and writes results into the result slice.
- **Spawn lowering**: each `spawn call(args...)` is lowered to: (1) evaluate
  arguments, (2) allocate a task descriptor struct containing the function
  pointer and argument values, (3) append the descriptor to the taskgroup's
  spawn array.
- **Closure spawns**: when the spawned expression is a closure call, the
  closure's environment struct is captured into the task descriptor, following
  the existing closure lowering pattern.
- **Channel lowering**: `chan[T]` lowers to an opaque `i64` handle (same
  pattern as maps and string builders). `chan_new` calls
  `yar_chan_new(elem_size, capacity)`. `chan_send` calls
  `yar_chan_send(handle, value_ptr)`. `chan_recv` calls
  `yar_chan_recv(handle, out_ptr)`. `chan_close` calls
  `yar_chan_close(handle)`.

### Runtime impact

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

**Timezone safety (~200 lines C):**

- `yar_tz_load(name)`: opens and parses the TZif binary file from the system
  timezone database (`/usr/share/zoneinfo/<name>` or `$ZONEINFO/<name>`).
  Returns an opaque GC-managed handle to an immutable transition table. Cached
  per timezone name.
- `yar_tz_convert(handle, unix_nanos)`: binary-searches the transition table
  to find the UTC offset at the given instant. Pure computation, no global
  state, fully thread-safe.
- On Windows: uses `SystemTimeToTzSpecificLocalTimeEx` with explicit
  `DYNAMIC_TIME_ZONE_INFORMATION` (thread-safe Win32 API).
- Replaces the current `setenv`/`tzset`/`localtime_r` approach in
  `time.date_in` and `time.from_date_in`.

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

Struct values passed to tasks are copied (value semantics). Struct pointers
passed to tasks share the pointed-to object — this is a potential data race and
the programmer's responsibility to avoid (or serialize access through a
channel).

### Enums

Enum values are copied into tasks. Payload data is copied. No interaction
issues.

### Slices

Slice values passed to tasks share the underlying backing storage. Concurrent
reads are safe. Concurrent mutation (including `append` which may reallocate)
is a data race. The intended pattern is: partition work by index range (each
task works on a disjoint slice range) or copy the relevant portion before
spawning.

### Maps

Map handles passed to tasks share the underlying map. Concurrent access is a
data race. The intended pattern is: use a "manager task" that owns the map and
accepts channel messages for lookups and mutations (actor pattern).

### Closures

Closures capture by value and are read-only. A closure spawned as a task gets
independent copies of all captured variables. This is the primary isolation
mechanism — no new rules needed.

### Control flow

`taskgroup` is an expression, not a statement. `break` and `continue` inside a
taskgroup body affect loops within the body, not enclosing loops. `return`
inside a taskgroup body returns from the enclosing function (the taskgroup is
abandoned; all already-spawned tasks are cancelled — their results are
discarded). `return` inside a spawned function returns from that task.

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

### Future: compile-time pointer sharing restriction

A future proposal may add a checker pass that rejects passing pointer-typed
values across task boundaries (through spawn arguments or closure capture).
This would make data races on pointer-shared objects a compile-time error
rather than a runtime race detector finding. This is deliberately excluded
from v1 to keep the checker changes manageable and to gather real-world usage
data on which patterns are common.

## 8a. Impact on Existing Builtins and Standard Library

This section documents every existing builtin and stdlib function, whether it
is safe under concurrent execution, and what runtime changes are required. The
audit covers the C runtime (`runtime_source.txt`), all host intrinsics, and
all stdlib packages.

### Runtime global state: mandatory changes

The C runtime has six global variables that control the garbage collector:

- `yar_gc_blocks` — linked list of all GC-managed allocations
- `yar_gc_bytes` — total allocated bytes
- `yar_gc_heap_target` — threshold for triggering collection
- `yar_gc_configured` — one-time init flag
- `yar_gc_collecting` — reentrancy guard
- `yar_gc_stack_top` — stack marker for root scanning

**All six are accessed without synchronization.** Every builtin and stdlib
function that allocates (via `yar_alloc` or `yar_alloc_zeroed`) races on
these globals. This is the single most critical change: add a GC mutex that
protects all allocation and collection operations. The stop-the-world barrier
(section 7) subsumes this mutex during collection.

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

**Safe per-handle (each task uses its own handle):**

| Builtin     | Shared handle risk                                                                              | Guidance                                                                                                           |
| ----------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `sb_new`    | Returns a new handle. Safe — each call creates an independent builder.                          | No change.                                                                                                         |
| `sb_write`  | Modifies builder buffer without synchronization. Two tasks writing the same builder corrupt it. | No runtime change. Document: builders must not be shared across tasks.                                             |
| `sb_string` | Reads and resets builder length. TOCTOU race if shared.                                         | No runtime change. Document: builders must not be shared.                                                          |
| `has`       | Reads map structure without locking.                                                            | No runtime change. Document: maps must not be shared across tasks, or access must be serialized through a channel. |
| `delete`    | Modifies map, may trigger rehash.                                                               | Same as `has`.                                                                                                     |

**Require runtime changes:**

| Builtin | Issue                                                                                                                                                           | Required change                                                                                                                                                                                                                                                   |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `print` | Concurrent writes to stdout interleave at the `fwrite` level. Individual `print` calls are not atomic for large strings.                                        | Add a stdout mutex in the runtime. Each `print` call acquires the mutex, writes the full string, releases. Output from different tasks may still be interleaved between calls, but individual messages are never torn.                                            |
| `panic` | Calls `exit(1)`. In a multi-threaded runtime, `exit()` runs atexit handlers and flushes stdio from the calling thread while other threads may still be running. | Change `panic` to: (a) set a global panic flag, (b) print the message under the stderr mutex, (c) call `_exit(1)` (immediate termination, no atexit handlers) to avoid races during shutdown. Alternatively, signal all scheduler threads to stop before exiting. |

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
| `process.run`         | Partially | The process creation and wait logic is safe (OS handles separate child processes). However, the POSIX implementation calls `getenv("TMPDIR")` for temp file creation — same fix as `fs.temp_dir` (use cached value). Parks task, submits to I/O thread pool.                                                |
| `process.run_inherit` | Partially | Parks task, submits to I/O thread pool. Multiple tasks running child processes with inherited stdio will produce interleaved output on stdout/stderr. This is the programmer's responsibility — the runtime cannot prevent it because the child processes write directly to the inherited file descriptors. |

### Host intrinsics: `stdio` package

| Function       | Safe? | Change needed                                                                                                                                        |
| -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `stdio.eprint` | No    | Same interleaving issue as `print`. Fix: add a stderr mutex in the runtime. Each `eprint` call acquires the mutex, writes the full string, releases. |

### Host intrinsics: `net` package

All network functions change behavior: they block the calling **task** instead
of the program, via integration with the netpoller.

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

### Host intrinsics: `time` package (proposed, not yet implemented)

The time package (proposal 0024) has not been implemented yet. The concurrency
impact should be addressed in its implementation:

| Function                                    | Thread-safe?    | Change needed                                                                                                                                                                                                                                  |
| ------------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `time.now`                                  | Yes             | `clock_gettime(CLOCK_REALTIME)` is thread-safe. No blocking.                                                                                                                                                                                   |
| `time.monotonic`                            | Yes             | `clock_gettime(CLOCK_MONOTONIC)` is thread-safe. No blocking.                                                                                                                                                                                  |
| `time.sleep`                                | Must change     | Currently blocks the program. Must park the task and register a timer with the netpoller (timerfd on Linux, `EVFILT_TIMER` on macOS). The task is woken when the timer fires. No thread consumed per sleeping task.                            |
| `time.date`                                 | Must change     | Proposal 0024 uses `gmtime_r`, which is thread-safe (writes to caller-provided buffer). Safe for concurrent calls. No blocking.                                                                                                                |
| `time.local_date`                           | Must change     | Proposal 0024 uses `localtime_r`, which acquires glibc's internal `tzset_lock`. Thread-safe for concurrent reads but serialized. Safe without changes, but consider replacing with the TZif parser for consistency.                            |
| `time.date_in`                              | **Must change** | Proposal 0024 uses `setenv("TZ",...) + tzset() + localtime_r()`. This is fundamentally not thread-safe (see resolved decision in section 12). Must be replaced with the TZif file parser (`yar_tz_load` + `yar_tz_convert`).                   |
| `time.from_date`                            | Must change     | Proposal 0024 uses `timegm` (or `_mkgmtime` on Windows). Both are thread-safe. No change needed.                                                                                                                                               |
| `time.from_local_date`                      | Must change     | Proposal 0024 uses `mktime`, which reads the `TZ` environment variable internally. Thread-safe on glibc (acquires `tzset_lock`), but subject to the same global `TZ` coupling. Consider replacing with the TZif parser for the local timezone. |
| `time.from_date_in`                         | **Must change** | Same as `time.date_in` — uses `setenv`/`tzset`. Must be replaced with TZif parser.                                                                                                                                                             |
| `time.format_*` / `time.parse_*`            | Yes             | Pure Yar code. Thread-safe (no global state, no blocking I/O). Allocations go through the GC mutex.                                                                                                                                            |
| `time.seconds` / `time.milliseconds` / etc. | Yes             | Pure arithmetic. No allocation, no side effects.                                                                                                                                                                                               |

**Recommendation for proposal 0024**: implement `time.date_in` and
`time.from_date_in` using the TZif file parser from the start, even though
the time package ships before concurrency. This avoids implementing the
`setenv`/`tzset` approach and then replacing it. The TZif parser works
correctly in single-threaded programs too — it is simply a better
implementation regardless of concurrency.

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
| `equal`, `not_equal`, `is_true`, `is_false`, `fail`, `log`, `has_failed` | Per-T        | Modify or read the `*testing.T` struct. Safe when each test has its own `T` instance (which is the case in the test runner). If `yar test` runs tests concurrently in the future, each test function receives a separate `T`. No runtime changes needed. |

### Summary of required runtime changes for existing code

**Critical (must be done before concurrency ships):**

1. **GC mutex**: protect `yar_alloc`, `yar_alloc_zeroed`, and
   `yar_gc_collect` with a mutex. Affects every function that allocates.
2. **`getenv` safety**: protect `yar_env_lookup` with a mutex that covers
   both the `getenv()` call and the copy of the returned string. Cache
   `TMPDIR` at startup for `yar_fs_temp_dir` and `yar_process_run`.
3. **stdout/stderr mutexes**: protect `yar_print` and `yar_eprint` so
   individual messages are not torn.
4. **`panic` termination**: change from `exit(1)` to immediate termination
   (`_exit(1)`) or coordinated shutdown to avoid races during exit.
5. **`net.close` safety**: deregister fd from netpoller and wake parked tasks
   with `error.Closed` before calling `close()`.
6. **`net.ensure_init` race**: replace static flag with `pthread_once`.
7. **`time.date_in` / `time.from_date_in`**: implement using TZif parser
   instead of `setenv`/`tzset` (resolved in section 12). This change should
   be made in the time package implementation (proposal 0024), not deferred
   to the concurrency implementation.

**Documentation-only (no runtime changes, behavior documented):**

8. Map handles shared across tasks are data races — document.
9. String builder handles shared across tasks are data races — document.
10. Socket handles should be owned by one task — document.
11. Slice mutation across tasks is a data race — document.
12. `sort.*` on shared slices is a data race — document.
13. `fs.write_file` to the same path from multiple tasks races — document.
14. `process.run_inherit` from multiple tasks interleaves child output —
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

- **Language surface**: moderate — 3 new keywords (`taskgroup`, `spawn`,
  `chan`), 1 new type (`chan[T]`), 4 new builtins (`chan_new`, `chan_send`,
  `chan_recv`, `chan_close`), 1 new expression form (`taskgroup`)
- **Parser complexity**: moderate — new expression, statement, and type forms
- **Checker complexity**: moderate — taskgroup type checking, spawn validation,
  channel type inference, `chan[T]` as a built-in parameterized type
- **Codegen complexity**: high — taskgroup lowering to runtime calls, task
  descriptor construction, channel handle lowering
- **Runtime complexity**: high — task scheduler, work-stealing queues, context
  switching, blocking I/O integration, channel implementation, GC
  modifications for multi-stack scanning
- **Diagnostics complexity**: moderate — errors for misplaced spawn, type
  mismatches in spawn and channel operations
- **Test burden**: high — concurrency correctness testing, race condition
  tests, channel edge cases, GC under concurrent load, blocking I/O
  integration tests
- **Documentation burden**: high — new language concepts, taskgroup semantics,
  channel usage patterns, data race rules, migration guide for existing
  blocking programs

This is the single highest-complexity proposal in Yar's history. The runtime
changes alone exceed all previous runtime additions combined. This is expected
— concurrency touches every layer of the system.

## 11. Why Now?

Concurrency is **not** proposed for immediate implementation. The status is
`exploring`, not `proposed` or `accepted`.

This proposal exists to:

1. **Establish the design direction** before implementation begins, so that
   decisions about the `time` package (0024), future stdlib additions, and
   runtime architecture can account for concurrency requirements.
2. **Identify the blast radius** — specifically, which existing components
   (GC, host intrinsics, `time.date_in`, `print`) need modification and how.
3. **Commit to structured concurrency** as the model, ruling out
   fire-and-forget goroutines and async/await early, so library design and
   user expectations align.
4. **Defer what can be deferred** — select, mutex, race detector, and
   compile-time pointer restrictions are explicitly future work, keeping v1
   of concurrency achievable.

The design should be finalized before the `time` package ships, because
`time.date_in`'s current `setenv`/`tzset` approach is incompatible with
concurrent execution and should be implemented with concurrency in mind from
the start.

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

**Implication for `return` in taskgroup body**: `return` in the body sets a
cancellation flag on all already-spawned tasks. Cancelled tasks are not
forcibly terminated — cancellation is cooperative (tasks must check a
cancellation mechanism to respond). The taskgroup still waits for all tasks to
complete before the scope exits. This matches every production implementation:
no system can forcibly cancel a task doing blocking I/O. Tasks that have
already produced side effects (file writes, network sends) are not rolled
back.

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

### Pointer sharing: no compile-time restriction in v1, race detector in v1

**Decision**: allow pointers across task boundaries in v1. Include a runtime
race detector (`-race` flag) as a v1 feature.

**Research findings**:

- Compile-time pointer restriction (rejecting pointers in `spawn` arguments
  and closure captures) would prevent data races but also prevent legitimate
  patterns: read-only shared data structures, disjoint-slice partitioning, and
  shared channel handles (channels are already safe for concurrent use).
  Distinguishing safe from unsafe pointer sharing requires Rust-level type
  system complexity (Send/Sync traits, borrow checker), which contradicts
  Yar's "small surface area" principle.
- The race detector is based on LLVM's ThreadSanitizer (TSan). TSan is an
  LLVM IR transformation pass that is language-agnostic — any LLVM frontend
  can use it. Yar already emits LLVM IR. Adding TSan instrumentation requires:
  (a) emitting `__tsan_read`/`__tsan_write` calls around memory accesses in
  codegen, (b) informing TSan about task creation and synchronization points
  (channel send/recv, taskgroup join) via `__tsan_acquire`/`__tsan_release`.
- TSan has zero false positives in happens-before mode. Overhead is 5-10x
  memory and 2-20x CPU — suitable for testing and CI, not production.
- Go ships its race detector as a core language feature (not a library) and
  this is cited as essential infrastructure. Uber found 2000+ races across
  46M lines of Go using it.
- Yar's conservative GC scanning must be annotated to TSan (the GC stop-the-
  world synchronization establishes happens-before edges that TSan must know
  about) to avoid false positives from GC scanning application memory.

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

### Timezone thread safety: parse TZif files in Yar runtime

**Decision**: implement a TZif file parser in the C runtime that reads the
system timezone database directly, producing an immutable timezone offset table.
Do not use `setenv`/`tzset`/`localtime_r` for named timezone conversion.

**Research findings**:

- `setenv` is fundamentally not thread-safe on any platform. POSIX does not
  require `getenv` to be thread-safe. glibc's `setenv` holds a lock but
  `getenv` does not, so concurrent `setenv` + `getenv` can crash. Apple libc
  frees old environment values, making the race even more dangerous. This is
  not fixable with a mutex in the Yar runtime because third-party C code
  (including libc itself) calls `getenv` without holding Yar's mutex.
- `newlocale`/`uselocale` (POSIX per-thread locale) only affect formatting
  (`strftime_l`), NOT timezone offset calculation. There is no
  `localtime_l()` in POSIX. This approach does not solve the problem.
- Go's approach: implements its own TZif parser entirely in Go code. Loads
  timezone files from the system (`/usr/share/zoneinfo/`) or from an embedded
  zip (~450 KB). `time.LoadLocation("America/New_York")` returns an immutable
  `*time.Location` that is fully thread-safe. Go never calls C's `localtime_r`
  or `tzset`.
- Rust's chrono (post-0.4.20): parses the OS timezone database natively in
  Rust, eliminating the soundness issue that led to CVE-2020-26235.
- NetBSD/FreeBSD provide `localtime_rz()`/`mktime_z()` with explicit timezone
  parameters, but these are not in POSIX and not portable.

**Design**: the Yar runtime implements `yar_tz_load(name)` which:

1. Opens `/usr/share/zoneinfo/<name>` (or `$ZONEINFO/<name>` if set).
2. Parses the TZif binary header to extract UTC/local transition times and
   offsets.
3. Returns an opaque handle to an immutable offset table (allocated via
   `yar_alloc`, GC-managed).
4. `yar_tz_convert(handle, unix_nanos)` binary-searches the transition table
   to find the offset at the given instant. Pure computation, no global state.
5. On Windows: use `SystemTimeToTzSpecificLocalTimeEx` with explicit
   `DYNAMIC_TIME_ZONE_INFORMATION` (thread-safe, takes timezone as parameter).
6. Timezone handles are cached — loading "America/New_York" twice returns the
   same handle.

This adds ~200 lines of C for TZif parsing (the format is simple: magic
number, header with counts, arrays of transition times and offset indices).
No embedded timezone database in v1 — rely on the system's
`/usr/share/zoneinfo`. Programs on systems without tzdata get `error.NotFound`,
which is explicit and handleable. Embedding can follow as a future enhancement.

### Child process waiting: I/O thread pool

**Decision**: `process.run` submits `waitpid` to the I/O thread pool. The
calling task is parked until the child exits.

**Research findings**:

- On Linux 5.3+, `pidfd_open` + epoll integration is clean (the pidfd becomes
  readable when the child exits). On macOS, kqueue's `EVFILT_PROC` with
  `NOTE_EXIT` handles this natively.
- However, `process.run` in Yar also captures stdout/stderr, which requires
  reading from pipes. Pipe reads can be integrated with the netpoller (pipes
  are pollable on both Linux and macOS).
- The simplest correct approach for v1: submit the entire `process.run`
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
| TZ file path      | /usr/share/zoneinfo                     | /usr/share/zoneinfo (or /var/db/timezone/zoneinfo) | SystemTimeToTzSpecificLocalTimeEx |

Assembly files: `yar_context_amd64.S` and `yar_context_arm64.S`, included
conditionally based on `YAR_ARCH`. The `setjmp`/`longjmp` fallback covers
any architecture without dedicated assembly.

### GC modifications for multi-task scanning

**Decision**: follow the Boehm GC multi-thread model — stop-the-world across
all OS threads, scan all task stacks, then resume.

**Research findings**:

- Yar's current GC is a conservative mark-and-sweep, stop-the-world collector.
  Adding concurrency does not require concurrent GC — the same STW approach
  works with multiple tasks. The critical change is scanning multiple stacks
  instead of one.
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
- Channel buffers are heap-allocated via `yar_alloc` and are part of the
  normal heap graph. No special treatment needed — the conservative scanner
  examines every word in live heap blocks.

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

Exploring. This proposal documents the intended concurrency design direction
for Yar. It is not yet proposed for implementation. The design should be
reviewed and refined before moving to `proposed` status.

## 15. Implementation Checklist

Not applicable at `exploring` status. When the proposal moves to `accepted`,
this section will be populated with specific implementation tasks covering:

### Phase 1: Runtime foundation (no language changes)

- [ ] `internal/runtime/yar_context_amd64.S` — amd64 context switch assembly
- [ ] `internal/runtime/yar_context_arm64.S` — arm64 context switch assembly
- [ ] `internal/runtime/yar_context.c` — context init, setjmp/longjmp fallback
- [ ] `internal/runtime/yar_scheduler.c` — thread pool, run queues, work
      stealing, task lifecycle
- [ ] `internal/runtime/yar_netpoll_epoll.c` — Linux network poller
- [ ] `internal/runtime/yar_netpoll_kqueue.c` — macOS/BSD network poller
- [ ] `internal/runtime/yar_netpoll_iocp.c` — Windows network poller
- [ ] `internal/runtime/yar_iopool.c` — blocking I/O thread pool
- [ ] `internal/runtime/yar_channel.c` — channel implementation
- [ ] `internal/runtime/yar_tz.c` — TZif parser, thread-safe timezone
      conversion (replaces setenv/tzset in time.date_in)
- [ ] `internal/runtime/runtime_source.txt` — GC multi-stack scanning,
      stop-the-world barrier, task stack registration
- [ ] `internal/runtime/runtime_source.txt` — modify yar*net*\* for non-blocking
      mode with poller integration
- [ ] `internal/runtime/runtime_source.txt` — modify yar*fs*_, yar*process*_
      for I/O thread pool submission

### Phase 2: Language surface

- [ ] `internal/token/token.go` — new keywords: `taskgroup`, `spawn`, `chan`
- [ ] `internal/ast/ast.go` — new AST nodes: `TaskgroupExpr`, `SpawnStmt`,
      `ChanType`
- [ ] `internal/parser/parser.go` — parse taskgroup, spawn, chan type
- [ ] `internal/checker/checker.go` — taskgroup type rules, spawn validation,
      channel builtin registration, `chan[T]` type
- [ ] `internal/codegen/llvm.go` — taskgroup lowering, channel handle codegen,
      spawn descriptor construction, task function wrapper generation

### Phase 3: Race detector

- [ ] `internal/codegen/llvm.go` — conditional TSan instrumentation when
      `-race` flag is active
- [ ] `internal/compiler/compiler.go` — `-race` flag handling, TSan runtime
      library linking
- [ ] `internal/runtime/yar_tsan.c` — TSan annotation helpers for GC barriers,
      channel operations, task creation/join

### Phase 4: Tests and documentation

- [ ] `testdata/concurrency_basic/main.yar` — basic taskgroup test
- [ ] `testdata/concurrency_channels/main.yar` — channel test
- [ ] `testdata/concurrency_errors/main.yar` — error propagation test
- [ ] `testdata/concurrency_net/main.yar` — concurrent TCP server test
- [ ] `testdata/concurrency_race/main.yar` — race detector validation
- [ ] `internal/compiler/compiler_test.go` — concurrency test functions
- [ ] `docs/context/domains/concurrency.md` — concurrency documentation
- [ ] `docs/context/domains/language-slice.md` — updated with taskgroup, spawn,
      chan
- [ ] `docs/context/domains/stdlib.md` — channel builtins
- [ ] `docs/context/platform/toolchain-runtime.md` — scheduler, poller, channel
      runtime, TZif parser, race detector
- [ ] `docs/context/summary.md` — updated capabilities
- [ ] `docs/YAR.md` — concurrency reference
- [ ] `LLM.txt` — concurrency reference
- [ ] `README.md` — updated feature list
