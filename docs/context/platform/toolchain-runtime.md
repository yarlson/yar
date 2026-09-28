# Toolchain Runtime

## External Dependency

- Native builds depend on `clang` being available on `PATH`.
- The Rust compiler emits textual LLVM IR and delegates machine-code generation
  and linking to `clang`.
- `crates/yar-compiler` is the Rust 2024 compiler rewrite path. Its current
  implemented slice covers token, diagnostic, AST, lexer, parser,
  package-graph loading/lowering, monomorphization, checker metadata, and a
  core function-body checker subset including ordinary calls, method calls,
  function literals, function-value calls, closure capture restrictions,
  taskgroups/spawn, typed channel builtins, interface calls/coercion, enum match
  validation, single-field enum positional constructors, loop
  `break`/`continue`, noreturn call flow, field/index/slice access, typed
  aggregate literals, map lookup propagation, and local `or` handling. Native
  build orchestration is available through `crates/yar-cli`, and GoReleaser
  release artifacts package that Rust CLI. `crates/yar-cli` provides `check`,
  `emit-ir`, `build`, host `run`, host `test`, `init`, and dependency
  manifest, lock, fetch, and update commands. The Rust LLVM emitter currently
  has clang-accepted coverage for
  every checked-in `testdata/**/main.yar` entry program. That coverage includes
  scalar/control-flow, strings, character literals, fixed arrays, structs,
  slice descriptors/literals/indexing/slicing/append, map literals,
  lookup/assignment, `len`, `has`, `delete`, `keys`, function literals,
  function values, closure calls, concrete method calls, interface calls,
  taskgroups/spawn wrappers for named functions and immediate inline literals,
  channel builtin runtime calls, direct `fs`, `process`, `env`, `stdio`, and
  `net` host-intrinsic runtime calls, captured closure environments, pointer
  operations, enum match lowering with payload constructors, and
  stdlib-internal builtins.
- `CC` overrides `clang`; shared process control preserves missing command names
  and applies one `YAR_BUILD_TIMEOUT_SECS` deadline to Cargo and clang.
- Tests use `YAR_TEST_TIMEOUT_SECS`; `yar run` programs remain unbounded after
  their build phase. Timed descendants are terminated before the command returns.
- On Windows, temporary executables produced by `Run` and `RunPath` use a
  `.exe` suffix so the OS can execute them.
- The default output name for `build` is `a.out` on Unix and `a.exe` on
  Windows.
- Native build paths link the Rust runtime through a strict target bundle.
  `YAR_RUNTIME_BUNDLE` selects a directory containing `yar-runtime.toml` and one
  static archive. Otherwise the CLI discovers `runtimes/<target-triple>/` next
  to the executable or, for host source builds, combines the checked-in target
  manifest with the Cargo-built workspace archive.

## Cross-Compilation

- `YAR_OS` and `YAR_ARCH` environment variables select the build target. If
  neither is set, the host platform is used. Both must be set together.
- The compiler maps the OS/arch pair to an LLVM target triple and passes
  `--target=<triple>` to `clang`. The generated LLVM IR also includes a
  `target triple` directive.
- Supported targets: `darwin/amd64`, `darwin/arm64`, `linux/amd64`,
  `linux/arm64`, `windows/amd64`.
- `windows/amd64` maps to `x86_64-pc-windows-gnu`, matching the Windows Rust
  release artifact and packaged `libyar_runtime.a`.
- Host ABIs outside the exact supported little-endian Darwin and GNU triples
  are rejected rather than relabeled as a compatible bundle target. The
  current Windows bundle and clang contract supports Windows GNU, not MSVC or
  GNU LLVM variants.
- Cross-compilation requires a `clang` installation that can target the
  requested platform, including the appropriate sysroot and system libraries.
- Rust CLI cross builds require a matching explicit `YAR_RUNTIME_BUNDLE` or an
  installed `runtimes/<target-triple>/` bundle. Workspace Cargo fallback is
  host-only.
- `yar run` rejects cross-compilation targets since the built binary cannot
  execute on the host.
- The bundle carries the ordered native libraries reported for its Rust
  staticlib target; the CLI validates their names and emits them after the
  archive. Release staging compares each checked-in list with
  `rustc --print native-static-libs` and fails on drift.

## Runtime Implementations

- `crates/yar-runtime` is the Rust 2024 runtime crate. It builds as an `rlib`
  for tests and as a `staticlib` for the native link boundary.
- Cargo compiles a small target-native C shim with the runtime archive. The shim
  spills callee-saved registers into the current frame before a thread stops
  for collection, reports the thread's outer stack boundary, and enumerates
  readable stack subranges; allocation, marking, sweeping, and runtime policy
  remain in Rust.
- The Rust crate exports C ABI symbols with the existing `yar_*` names for the
  helpers it has ported. The ported surface currently includes low-level I/O,
  trap, allocation, bounds-checking, string conversion / concatenation, map,
  string-builder, taskgroup, channel, argv capture, environment lookup,
  child-process execution, and filesystem and TCP networking helpers.
- Host `build`, `run`, and `test` commands can build `crates/yar-runtime` with
  Cargo and validate the checked-in target manifest against the resulting
  archive when no explicit or packaged bundle is available. The CLI resolves
  one `CARGO_TARGET_DIR`, passes it to Cargo, and loads the archive from that
  same directory.
- Bundle format, exact target triple, runtime ABI, and compiler compatibility
  are independent integer epochs and must all match. The archive must be one
  relative regular file; system-library names are validated while order and
  duplicates are preserved. Legacy `YAR_RUNTIME_ARCHIVE` configuration is
  rejected with migration guidance.
- The runtime uses portable Rust standard-library filesystem, environment, and
  TCP APIs where their contracts match Yar. Platform-specific Rust branches are
  limited to behavior that actually differs, including path byte conversion,
  GC stack discovery, Unix process groups and signals, and Windows Job Objects.
- Source-level child execution uses explicit argument-owned deadlines and
  cancellation. Captured runs drain both pipes under independent byte caps.
  Timeout, cancellation, or a cap breach synchronously terminates ordinary
  descendants and reaps the leader before returning. This contract is
  independent of deadlines for subprocesses owned by the Rust CLI.
- Concurrency uses portable Rust native threads on Linux, macOS, and Windows
  GNU. CI executes taskgroup and channel programs on Windows in addition to
  building the target runtime bundle.
- Conservative stack scanning on Windows records each registered thread's
  OS-reported stack limit and walks only committed, readable regions reported
  by `VirtualQuery`; guard and inaccessible regions are never dereferenced.
  This runtime contract requires Windows 8 or newer.

## Runtime Surface

- Runtime ABI 5 passes every aggregate input and output through explicit
  caller-owned pointers. Generated LLVM therefore does not depend on
  target-specific aggregate argument or return conventions at the Rust/C
  boundary. Runtime calls read input slots during the call and initialize
  output slots before returning successfully.

### I/O and Control

- `yar_print(const char *data, long long len)` writes string data to stdout when
  the length is positive.
- `yar_panic(const char *data, long long len)` writes and flushes the message
  on a single-threaded fatal path, omits output while tasks are unjoined, and
  immediately exits with status `1`.
- `yar_eprint(const char *data, long long len)` writes string data to stderr
  when the length is positive and flushes stderr.
- The generated native `main` wrapper accepts `argc` / `argv`, records a
  stack-top pointer through `yar_gc_init_stack_top(void *stack_top)`, forwards
  arguments to `yar_set_args(int32_t argc, char **argv)`, and then calls user
  `yar.main()`.
- The generated unhandled-error path uses `yar_print`, so unhandled `main`
  errors currently surface on stdout rather than stderr.

### Allocation

- `yar_gc_init_stack_top(void *stack_top)` registers the main thread as a
  collector mutator with that outer stack boundary.
- `yar_alloc(long long size, const YarGcDescriptor *descriptor)` returns zeroed
  collector-managed storage, may first stop at a safepoint or run a collection,
  and traps on invalid size or allocation failure. A null descriptor marks the
  object pointer-free; its contents are never scanned.
- A descriptor is `{ i64 stride, i64 count, [count x i64] offsets }`. The
  collector repeats the element layout across the whole object and treats each
  listed word offset as a candidate pointer. Codegen emits one private constant
  per distinct layout; the runtime keeps its own static descriptors for maps,
  channel tokens, string arrays, and directory entries, and uses a one-word
  layout that visits every aligned word when it does not know a layout.
- `yar_gc_safepoint_requested` is an exported 32-bit flag. Generated loops load
  it once per iteration and call `yar_gc_safepoint(void)` when it is non-zero.
- `yar_gc_collect(void)` runs a full stop-the-world collection from a
  registered thread.
- `yar_trap_oom(void)` terminates with `runtime failure: out of memory` on
  stderr and exit status `1`.
- `YAR_GC_HEAP_TARGET_BYTES` overrides the minimum allocation budget between
  collections; invalid or empty values use the 4 MiB default. After each
  collection the budget becomes the larger of that minimum and the surviving
  live bytes, so the heap may grow to about twice its live size.

### Collector

- The heap consists of 64 KiB pages. Objects up to 32 KiB use 50 size classes;
  larger objects get dedicated page-aligned spans. A two-level page map
  resolves any address, including interior pointers, to its object in constant
  time. Pages record live and mark bits and one layout descriptor per slot
  outside the object memory.
- Each thread allocates from its own current page per size class without
  locking. Pages are taken from or returned to shared lists under one heap
  lock, and allocation pacing is counted per page rather than per object.
- Every thread that runs Yar code is a registered mutator. A collection sets the
  safepoint flag and waits until every mutator is stopped: at an allocation,
  at a loop safepoint, or inside a blocking runtime operation such as a channel
  wait, task join, socket wait, file or process I/O, or contended resource-lock
  acquisition. Stopped threads publish the low end of their spilled frame.
- Roots are the aligned words of every stopped thread's stack, including
  spilled registers, plus explicit runtime roots for spawned task contexts and
  pending task results. Heap objects are traced precisely from their
  descriptors. Marking is parallel when the previous live heap was at least
  8 MiB.
- Sweeping copies mark bits into live bits per page during the pause. Empty
  pages return to the allocator (a bounded number stay cached for reuse), and
  freed slots are zeroed when they are reused. Finalizers run for unreachable
  runtime objects such as channel tokens before sweeping.
- The collector is non-moving. Conservative stack words may delay reclamation,
  and collection timing is not user-visible.
- Runtime code never waits for a collection to finish while holding a lock that
  a running mutator could block on, and allocates managed memory only after
  releasing locks that other mutators acquire outside a blocking region.

### Integer Arithmetic Runtime

- `yar_i32_divrem_check(int32_t dividend, int32_t divisor)` guards generated
  `i32` division and remainder operations.
- `yar_i64_divrem_check(int64_t dividend, int64_t divisor)` provides the same
  guard for `i64`.
- Both helpers terminate on a zero divisor or the signed overflow pair `MIN`
  and `-1`, before generated code executes LLVM `sdiv` or `srem`.

### Pointer Runtime

- `yar_pointer_check(const void *pointer)` terminates with
  `runtime failure: nil pointer dereference` when generated code attempts to
  dereference a null pointer.

### Concurrency Runtime

- `yar_taskgroup_new(int64_t elem_size, const YarGcDescriptor *descriptor)`
  allocates a taskgroup handle; the descriptor describes one result element.
- Each spawn allocates a managed result slot and roots it together with the
  task context until the taskgroup is joined. Spawned threads register as
  collector mutators for their lifetime.
- `yar_taskgroup_spawn(void *group, void *entry, void *ctx)` records one task
  and starts it on a native OS thread immediately.
- `yar_taskgroup_wait(void *group, YarSlice *out)` joins all started tasks and
  writes a runtime-managed result slice whose element order matches spawn order.
- `yar_chan_new(int64_t elem_size, int32_t capacity, const YarGcDescriptor
  *descriptor)` allocates a bounded FIFO channel whose buffer is a managed
  object referenced by the channel token, so buffered values are traced as
  ordinary heap contents.
- `yar_chan_send(void *handle, const void *value_ptr)` blocks while the channel
  buffer is full and returns a non-zero status when the channel is closed.
- `yar_chan_recv(void *handle, void *out_ptr)` blocks while the channel is
  empty and open, and returns a non-zero status when the channel is closed and
  drained.
- `yar_chan_close(void *handle)` closes the channel and wakes blocked senders
  and receivers.

### Array and Slice Runtime

- `yar_array_index_check(long long index, long long len)` traps on out-of-range
  fixed-array indexing before generated code computes an element address.
- `yar_slice_index_check(long long index, long long len)` traps on out-of-range
  slice indexing.
- `yar_slice_range_check(long long start, long long end, long long len)` traps
  on invalid slice ranges.

### String Runtime

- `yar_str_equal(const char *a_ptr, long long a_len, const char *b_ptr,
long long b_len)` compares two strings by length then bytes.
- `yar_str_concat(const char *a_ptr, long long a_len, const char *b_ptr,
long long b_len, YarStr *out)` allocates and writes a new string containing the
  concatenation of both inputs.
- `yar_str_index_check(long long index, long long len)` traps on out-of-range
  string indexing.
- `yar_str_from_byte(int32_t value, YarStr *out)` writes a one-byte string and
  traps if the value is outside `0..255`.
- `yar_to_str_i32(int32_t value, YarStr *out)` writes a signed 32-bit integer
  as a decimal string.
- `yar_to_str_i64(int64_t value, YarStr *out)` writes a signed 64-bit integer
  as a decimal string.

### Runtime Handle Registry

- String builders, streaming files, TCP listeners, and TCP connections use
  positive process-local opaque `i64` tokens rather than exposed native
  addresses. Network tokens are internal to typed, share-safe `Conn` and
  `Listener` values.
- A token uses a fixed high marker plus a nonzero 31-bit generation and a
  one-based 31-bit slot number. The marker keeps handle-shaped integers outside
  ordinary managed-address ranges used by conservative collection. Removing an
  entry advances its generation before placing the slot on the free list, so
  reuse changes the full token and stale generations cannot resolve to a newer
  resource. A slot is retired after its maximum generation is removed rather
  than wrapping to a previously issued token.
- Registry lookup and removal validate both generation and expected resource
  kind. Stale and wrong-kind attempts do not consume or alter the live entry, so
  a listener token cannot be used as a connection, file, or string builder.
- Registry lookup returns synchronized per-resource state and releases the
  registry lock before filesystem or network I/O. Operations on one handle do
  not hold the registry lock across blocking work.
- Network close removes the ID so later lookup fails, wakes blocked
  accept/read/write calls with `Closed`, and waits for operation and resource
  release. File close remains non-interrupting and performs no implicit
  durability sync.
- Unknown, stale-generation, and wrong-kind file or network tokens map to
  `error.Closed`.
  Invalid string-builder IDs terminate with
  `runtime failure: invalid string builder`.
- The string-builder ABI uses `i64` directly: `yar_sb_new()` returns an ID;
  `yar_sb_write`, `yar_sb_string`, `yar_sb_finish`, and `yar_sb_discard` accept
  that ID without pointer/integer conversion in generated IR. `yar_sb_string`
  writes through an explicit output pointer and retains the handle;
  `yar_sb_finish` writes through an output pointer and consumes the handle;
  `yar_sb_discard` consumes it without allocating a result.
- Registry validation is a runtime safety boundary, not nominal typing. Source
  `i64` values still carry no compiler-visible handle kind or provenance.

### Filesystem Runtime

- `yar_fs_read_file(const yar_str *path, yar_str *out)` reads a whole file into one
  runtime-managed string and returns a stable filesystem status code.
- `yar_fs_write_file(const yar_str *path, const yar_str *data)` writes one whole file and
  returns a stable filesystem status code.
- `yar_fs_read_dir(const yar_str *path, yar_slice *out)` returns a slice of
  `fs.DirEntry`-layout values (`name`, `is_dir`) and a stable filesystem status
  code.
- `yar_fs_stat(const yar_str *path, int32_t *kind_out)` classifies a path as file,
  directory, or other.
- `yar_fs_mkdir_all(const yar_str *path)` creates a directory tree.
- `yar_fs_remove_all(const yar_str *path)` recursively removes a file or directory
  tree.
- `yar_fs_temp_dir(const yar_str *prefix, yar_str *out)` creates one temporary
  directory under `TMPDIR` or `/tmp`.
- `yar_fs_open_read(const yar_str *path, int64_t *out)` opens a file for streaming
  reads and returns an opaque registry ID.
- `yar_fs_open_write(const yar_str *path, int64_t *out)` creates or truncates a file
  for streaming writes and returns an opaque registry ID.
- `yar_fs_read_handle(int64_t handle, int32_t max_bytes, yar_str *out)` reads
  up to `max_bytes` from an open file handle and returns empty string on EOF.
- `yar_fs_write_handle(int64_t handle, const yar_str *data, int32_t *out)` writes data
  to an open file handle and returns bytes written.
- `yar_fs_close_handle(int64_t handle)` closes an open file handle.
- Runtime filesystem status codes map in code generation to package-owned `fs`
  declarations (`NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidPath`,
  `InvalidArgument`, and `IO`) plus compiler-owned `error.Closed`. Runtime
  status values do not change.
- Filesystem operations are implemented with Rust `std::fs`, `std::path`, and
  `std::env` APIs. The small platform-specific boundary converts Yar path bytes
  to and from Unix `OsString` bytes or Windows UTF-8 strings.
- Path normalization relies on the `path` stdlib package rather than a
  platform-specific separator API. The runtime adjusts separator handling
  per-platform where needed.

### Process / Environment Runtime

- `yar_process_args(yar_slice *out)` copies the full host argument vector,
  including `argv[0]`, into a runtime-managed `[]str`.
- `yar_process_run` receives argv, validated timeout/capture limits, a close-only
  cancellation signal, and a result out-pointer. It drains bounded stdout and
  stderr concurrently.
- `yar_process_run_inherit` receives argv, a timeout, a cancellation signal,
  and an exit-code out-pointer while inheriting stdin/stdout/stderr.
- `yar_env_lookup(const yar_str *name, yar_str *out)` looks up one environment
  variable and returns a stable host-process status code.
- Host-process status codes map in code generation to package-owned
  `process` declarations (`NotFound`, `PermissionDenied`, `InvalidArgument`,
  `Timeout`, `LimitExceeded`, `Cancelled`, and `IO`) or the corresponding
  `env` declarations for environment lookup. Runtime status values do not
  change.
- Process launch and waiting use the shared Rust process-control layer. Captured
  runs drain bounded stdout/stderr pipes concurrently. Controlled Unix children
  run in an operation-owned process group; controlled Windows children are
  assigned to a kill-on-close Job Object before they resume. Environment lookup
  remains a separate Rust host operation.

### Networking Runtime

- `yar_net_listen(const yar_str *host, int32_t port, int64_t *out)` binds and listens
  on a TCP address. Empty host means the IPv4 wildcard address. It returns an
  internal listener registry ID via the out-pointer.
- `yar_net_accept(int64_t listener, int64_t *out)` blocks until a connection
  arrives and returns an opaque connection registry ID.
- `yar_net_listener_addr(int64_t listener, yar_net_addr *out)` returns the
  bound address of a listener socket.
- `yar_net_close_listener(int64_t listener)` closes a listener socket.
- `yar_net_connect(const yar_str *host, int32_t port, int64_t *out)` performs TCP
  connect with DNS resolution and returns a connection registry ID.
- `yar_net_read(int64_t conn, int32_t max_bytes, yar_str *out)` reads up to
  `max_bytes` from a connection. Returns empty string on EOF.
- `yar_net_write(int64_t conn, const yar_str *data, int32_t *out)` performs one
  host write and returns its exact byte count, which may be short.
- `yar_net_close(int64_t conn)` closes a connection socket.
- `yar_net_shutdown_write(int64_t conn)` serializes with writes and shuts down
  only the socket's write half, leaving reads available until close.
- `yar_net_local_addr(int64_t conn, yar_net_addr *out)` returns the local
  address of a connection via `getsockname`.
- `yar_net_remote_addr(int64_t conn, yar_net_addr *out)` returns the remote
  address of a connection via `getpeername`.
- `yar_net_set_read_deadline(int64_t conn, int32_t millis)` sets the relative
  timeout captured by the next read operation. Zero disables the timeout.
- `yar_net_set_write_deadline(int64_t conn, int32_t millis)` sets the relative
  timeout captured by the next write operation. Zero disables the timeout.
- `yar_net_set_read_deadline_after(int64_t conn, int32_t millis)` stores one
  fixed deadline measured from the call and shared by later reads. Zero
  disables it.
- `yar_net_set_write_deadline_after(int64_t conn, int32_t millis)` stores one
  fixed deadline measured from the call and shared by later writes. Zero
  disables it.
- `yar_net_resolve(const yar_str *host, int32_t port, yar_net_addr *out)` performs
  DNS resolution and returns the first IPv4 or IPv6 address; resolver failure
  maps to `NotFound`.
- Network ABI entry points operate on compiler-internal IDs. The runtime permits
  one reader and one writer concurrently and serializes same-direction calls.
  Reads accept 1 through 67,108,864 bytes inclusive. Writes perform one host
  write and return the exact, possibly short, count.
- Listener and connection sockets are nonblocking internally. A call that would
  block polls its close marker and operation-local timeout with adaptive
  1-through-64-millisecond waits while retaining the blocking source-level
  contract for that native task thread. This is a portability mechanism, not a
  high-scale readiness poller; each blocked call still owns one native thread.
- Read/write timeouts are relative per-operation socket timeouts. Fixed
  deadline-after values persist across operations, and the earlier applicable
  deadline wins. Updating either form need not interrupt a syscall already
  running. DNS and connection creation are synchronous host calls and cannot be
  interrupted before a handle exists.
- All networking functions return unchanged `i32` status codes that map in code
  generation to package-owned `net` declarations (`ConnectionRefused`,
  `Timeout`, `AddrInUse`, `ConnectionReset`, `NotFound`, `PermissionDenied`,
  `InvalidArgument`, and `IO`) plus compiler-owned `error.Closed`.
- Networking is implemented with Rust `std::net`. Listener and connection
  sockets are nonblocking internally; the runtime provides the blocking Yar
  contract through adaptive polling of readiness, close state, and
  operation-local timeouts. Windows runtime bundles include `ws2_32` in the
  Rust static library's ordered native-library contract.

### Map Runtime

- `yar_map_new(int32_t key_kind, int32_t key_size, int32_t value_size)`
  allocates a new open-addressed hash map with initial capacity `8`.
- `yar_map_set(void *map_ptr, const void *key, const void *value)` inserts or
  replaces an entry, growing the table at 75% load.
- `yar_map_get(void *map_ptr, const void *key, void *value_out)` looks up a
  key, copies the value into `value_out`, and returns `1` if found or `0`
  otherwise.
- `yar_map_has(void *map_ptr, const void *key)` returns `1` if the key exists,
  `0` otherwise.
- `yar_map_delete(void *map_ptr, const void *key)` removes the entry for a key
  and rehashes forward entries to preserve linear probing.
- `yar_map_len(void *map_ptr)` returns the current entry count.
- `yar_map_keys(void *map_ptr, YarSlice *out)` writes a snapshot slice
  containing the current keys.
- Key kinds are passed from code generation as integer constants: `bool` (`0`),
  `i32` (`1`), `i64` (`2`), `str` (`3`).
- Maps use FNV-1a hashing with linear probing and power-of-two capacity.

## Allocation Boundary

- The compiler emits declarations for shared runtime allocation helpers and
  uses them for user-visible pointer-supporting features.
- This establishes one allocation/trap boundary for heap-backed features rather
  than separate per-feature runtime entry points.
- Slice literals, `append`, pointer composite literals, map allocations, and
  local or parameter storage used by address-taking all reuse that same
  allocation boundary.
- The Rust runtime reclaims unreachable managed allocations. Conservative stack
  words may delay reclamation, and collection timing is not user-visible.
- Every allocation site passes the pointer layout of the allocated type, so
  pointer-free data such as strings and scalar slices is never scanned.
- Pointer composite literals lower by allocating storage for the pointed-to
  value and storing the literal into that storage.
- Map creation, growth, string concatenation results, host-returned strings,
  process argv snapshots, and filesystem directory-entry snapshots all allocate
  through the same runtime helpers.
- Allocation failure is treated as an unrecoverable runtime failure, not a YAR
  `error` value.

## Testing Boundary

- Compiler tests build real native executables and execute them.
- The test suite validates successful output, propagated unhandled errors,
  `panic`, `i64` compilation, slice behavior and traps, pointer behavior, enum
  definition and exhaustive `match`, map operations, control flow and aggregate
  programs, package-owned error identity and visibility, the `?` /
  `or |err| { ... }` error-sugar paths, multi-package imports, string
  operations (including indexing, slicing, and concatenation
  edge cases), stdlib imports, host filesystem/path behavior, host
  process/environment behavior, CC override behavior, internal builtin
  rejection, the embedded allocation/helper surface, a tight-heap
  garbage-collection churn fixture, and the `yar test` command with passing and
  failing test fixtures through the same `clang` boundary used by the CLI.
