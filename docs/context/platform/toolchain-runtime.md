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
  uses `setjmp` to expose ABI-preserved register roots to the Rust collector;
  allocation, marking, sweeping, and runtime policy remain in Rust.
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
- The Rust runtime uses `#[cfg(...)]` platform modules and branches to select
  POSIX or Win32 implementations at compile time.
- Concurrency support is currently implemented only on POSIX targets. Windows
  builds compile, but the concurrency runtime entry points fail with an
  explicit runtime error when called.

## Runtime Surface

### I/O and Control

- `yar_print(const char *data, long long len)` writes string data to stdout when
  the length is positive.
- `yar_panic(const char *data, long long len)` writes the message to stderr,
  flushes stderr, and exits with status `1`.
- `yar_eprint(const char *data, long long len)` writes string data to stderr
  when the length is positive and flushes stderr.
- The generated native `main` wrapper accepts `argc` / `argv`, records a
  stack-top pointer through `yar_gc_init_stack_top(void *stack_top)`, forwards
  arguments to `yar_set_args(int32_t argc, char **argv)`, and then calls user
  `yar.main()`.
- The generated unhandled-error path uses `yar_print`, so unhandled `main`
  errors currently surface on stdout rather than stderr.

### Allocation

- `yar_gc_init_stack_top(void *stack_top)` registers the outer main-stack
  boundary used by conservative collection.
- `yar_gc_collect(void)` captures ABI-preserved registers, scans the main stack,
  live channel slots, and transitively reachable managed blocks, then sweeps
  unreachable blocks. Calls are deferred while spawned results are unjoined.
- `yar_trap_oom(void)` terminates with `runtime failure: out of memory` on
  stderr and exit status `1`.
- `yar_alloc(long long size)` allocates initialized collector-managed storage,
  may trigger collection when the heap target is crossed, and traps on invalid
  size or allocation failure.
- `yar_alloc_zeroed(long long size)` allocates zeroed runtime-managed storage
  and traps on invalid size or allocation failure.
- Runtime configuration may override the initial collection threshold; invalid
  or empty values use the 1 MiB default.
- The collector is conservative, non-moving, and recognizes exact, interior,
  and unaligned pointer representations.

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

- `yar_taskgroup_new(int32_t elem_size)` allocates a taskgroup handle.
- `yar_taskgroup_spawn(void *group, void *entry, void *ctx)` records one task
  and starts it on a native POSIX thread immediately.
- `yar_taskgroup_wait(void *group)` joins all started tasks and returns a
  runtime-managed result slice whose element order matches spawn order.
- `yar_chan_new(int32_t elem_size, int32_t capacity)` allocates a bounded FIFO
  channel.
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
long long b_len)` allocates and returns a new string containing the
  concatenation of both inputs.
- `yar_str_index_check(long long index, long long len)` traps on out-of-range
  string indexing.
- `yar_str_from_byte(int32_t value)` allocates a one-byte string from a byte
  value and traps if the value is outside `0..255`.
- `yar_to_str_i32(int32_t value)` formats a signed 32-bit integer as a
  decimal string.
- `yar_to_str_i64(int64_t value)` formats a signed 64-bit integer as a
  decimal string.

### Runtime Handle Registry

- String builders, streaming files, TCP listeners, and TCP connections use
  positive process-local `i64` registry IDs rather than exposed native
  addresses.
- IDs increase monotonically and are never reused within a process. Registry
  lookup validates the expected resource kind, so a listener ID cannot be used
  as a connection, file, or string builder.
- Registry lookup returns synchronized per-resource state and releases the
  registry lock before filesystem or network I/O. Operations on one handle do
  not hold the registry lock across blocking work.
- Explicit file and network close removes the ID so later lookup fails, then
  waits for any operation holding the per-resource lock before taking and
  dropping the host resource. Close does not interrupt blocking I/O.
- Unknown, stale, and wrong-kind file or network IDs map to `error.Closed`.
  Invalid string-builder IDs terminate with
  `runtime failure: invalid string builder`.
- The string-builder ABI uses `i64` directly: `yar_sb_new()` returns an ID,
  while `yar_sb_write` and `yar_sb_string` accept that ID without pointer/integer
  conversion in generated IR.
- Registry validation is a runtime safety boundary, not nominal typing. Source
  `i64` values still carry no compiler-visible handle kind or provenance.

### Filesystem Runtime

- `yar_fs_read_file(yar_str path, yar_str *out)` reads a whole file into one
  runtime-managed string and returns a stable filesystem status code.
- `yar_fs_write_file(yar_str path, yar_str data)` writes one whole file and
  returns a stable filesystem status code.
- `yar_fs_read_dir(yar_str path, yar_slice *out)` returns a slice of
  `fs.DirEntry`-layout values (`name`, `is_dir`) and a stable filesystem status
  code.
- `yar_fs_stat(yar_str path, int32_t *kind_out)` classifies a path as file,
  directory, or other.
- `yar_fs_mkdir_all(yar_str path)` creates a directory tree.
- `yar_fs_remove_all(yar_str path)` recursively removes a file or directory
  tree.
- `yar_fs_temp_dir(yar_str prefix, yar_str *out)` creates one temporary
  directory under `TMPDIR` or `/tmp`.
- `yar_fs_open_read(yar_str path, int64_t *out)` opens a file for streaming
  reads and returns an opaque registry ID.
- `yar_fs_open_write(yar_str path, int64_t *out)` creates or truncates a file
  for streaming writes and returns an opaque registry ID.
- `yar_fs_read_handle(int64_t handle, int32_t max_bytes, yar_str *out)` reads
  up to `max_bytes` from an open file handle and returns empty string on EOF.
- `yar_fs_write_handle(int64_t handle, yar_str data, int32_t *out)` writes data
  to an open file handle and returns bytes written.
- `yar_fs_close_handle(int64_t handle)` closes an open file handle.
- Runtime filesystem status codes map in code generation to stable YAR error
  names: `NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidPath`,
  `InvalidArgument`, `Closed`, and `IO`.
- On POSIX, the implementation uses `stat`, `opendir`, `mkdir`, `remove`, and
  `mkstemp`. On Windows, the implementation uses Win32 APIs
  (`CreateFileA`, `FindFirstFileA`, `CreateDirectoryA`,
  `GetEnvironmentVariableA`, and related functions).
- Path normalization relies on the `path` stdlib package rather than a
  platform-specific separator API. The runtime adjusts separator handling
  per-platform where needed.

### Process / Environment Runtime

- `yar_process_args(yar_slice *out)` copies the full host argument vector,
  including `argv[0]`, into a runtime-managed `[]str`.
- `yar_process_run(const yar_slice *argv, yar_process_result *out)` launches
  one child process, captures stdout/stderr into runtime-managed strings, and
  returns a stable host-process status code.
- `yar_process_run_inherit(const yar_slice *argv, int32_t *exit_code_out)`
  launches one child with inherited stdin/stdout/stderr and returns a stable
  host-process status code.
- `yar_env_lookup(yar_str name, yar_str *out)` looks up one environment
  variable and returns a stable host-process status code.
- Host-process status codes map in code generation to stable YAR error names:
  `NotFound`, `PermissionDenied`, `InvalidArgument`, and `IO`.
- On POSIX, the implementation uses `fork`, `execvp`, `waitpid`, `mkstemp`,
  and `getenv`. On Windows, the implementation uses `CreateProcessA`,
  `GetEnvironmentVariableA`, and Win32 pipe/file handles. Both paths capture
  child stdout/stderr through temporary files.

### Networking Runtime

- `yar_net_listen(yar_str host, int32_t port, int64_t *out)` binds and listens
  on a TCP address. Empty host means all interfaces. Returns an opaque listener
  registry ID via the out-pointer.
- `yar_net_accept(int64_t listener, int64_t *out)` blocks until a connection
  arrives and returns an opaque connection registry ID.
- `yar_net_listener_addr(int64_t listener, yar_net_addr *out)` returns the
  bound address of a listener socket.
- `yar_net_close_listener(int64_t listener)` closes a listener socket.
- `yar_net_connect(yar_str host, int32_t port, int64_t *out)` performs TCP
  connect with DNS resolution and returns a connection registry ID.
- `yar_net_read(int64_t conn, int32_t max_bytes, yar_str *out)` reads up to
  `max_bytes` from a connection. Returns empty string on EOF.
- `yar_net_write(int64_t conn, yar_str data, int32_t *out)` writes all data
  to a connection. Returns bytes written.
- `yar_net_close(int64_t conn)` closes a connection socket.
- `yar_net_local_addr(int64_t conn, yar_net_addr *out)` returns the local
  address of a connection via `getsockname`.
- `yar_net_remote_addr(int64_t conn, yar_net_addr *out)` returns the remote
  address of a connection via `getpeername`.
- `yar_net_set_read_deadline(int64_t conn, int32_t millis)` sets a read
  timeout via `SO_RCVTIMEO`. Zero disables the timeout.
- `yar_net_set_write_deadline(int64_t conn, int32_t millis)` sets a write
  timeout via `SO_SNDTIMEO`. Zero disables the timeout.
- `yar_net_resolve(yar_str host, int32_t port, yar_net_addr *out)` performs
  DNS resolution and returns the first resolved address.
- All networking functions return `i32` status codes that map in code generation
  to stable YAR error names: `ConnectionRefused`, `Timeout`, `AddrInUse`,
  `ConnectionReset`, `NotFound`, `PermissionDenied`, `InvalidArgument`, `IO`,
  and `Closed`.
- On POSIX, the implementation uses BSD sockets (`socket`, `bind`, `listen`,
  `accept`, `connect`, `recv`, `send`, `getaddrinfo`, `getsockname`,
  `getpeername`). SIGPIPE is suppressed via `signal(SIGPIPE, SIG_IGN)` and
  `SO_NOSIGPIPE` on macOS. On Windows, the implementation uses Winsock2
  (`WSAStartup`, `socket`, `bind`, `listen`, `accept`, `connect`, `recv`,
  `send`, `getaddrinfo`, `closesocket`).
- Windows builds link `-lws2_32` for Winsock2 support.

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
- `yar_map_keys(void *map_ptr)` returns a snapshot slice containing the current
  keys.
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
- The Rust runtime reclaims unreachable managed allocations. Conservative false
  positives may delay reclamation, and collection timing is not user-visible.
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
  programs, the `?` / `or |err| { ... }` error-sugar paths, multi-package
  imports, string operations (including indexing, slicing, and concatenation
  edge cases), stdlib imports, host filesystem/path behavior, host
  process/environment behavior, CC override behavior, internal builtin
  rejection, the embedded allocation/helper surface, a tight-heap
  garbage-collection churn fixture, and the `yar test` command with passing and
  failing test fixtures through the same `clang` boundary used by the CLI.
