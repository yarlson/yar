# Yar

![Yar](assets/yar-banner.jpg)

Yar is a compiled programming language that targets native executables through
LLVM. It has explicit error handling, closed enums with exhaustive matching,
generics, interfaces, structured concurrency, and runtime-managed heap
allocation with no exceptions, no implicit coercions, and no hidden control
flow.

Read [The Yar Code](docs/language/the-yar-code.md) before you write a line.

## Quick look

```
package main

import "std/strings"

fn greet(name str) !void {
    if strings.contains(name, " ") {
        return error.InvalidName
    }
    print("hello, " + name + "\n")
}

fn main() !i32 {
    greet("world")?
    return 0
}
```

```
$ yar run greet.yar
hello, world
```

Enums are closed. `match` is exhaustive:

```
package main

enum Shape {
    case Circle { radius i32 }
    case Rect { w i32, h i32 }
}

fn area(s Shape) i32 {
    match s {
        case Shape.Circle { radius } {
            return radius * radius * 3
        }
        case Shape.Rect { w, h } {
            return w * h
        }
    }
}

fn main() i32 {
    s := Shape.Rect{w: 4, h: 5}
    print("area: " + to_str(area(s)) + "\n")
    return 0
}
```

```
$ yar run shapes.yar
area: 20
```

Structured concurrency uses `taskgroup` and typed bounded channels:

```
package main

fn square(v i32, out chan[i32]) void {
    chan_send(out, v * v) or |err| {
        return
    }
}

fn main() i32 {
    out := chan_new[i32](2)

    taskgroup []void {
        spawn square(2, out)
        spawn square(3, out)
    }

    a := chan_recv(out) or |err| {
        return 1
    }
    b := chan_recv(out) or |err| {
        return 1
    }
    print(to_str(a + b) + "\n")
    return 0
}
```

```
$ yar run squares.yar
13
```

## Design

- Functions that can fail return `!T`. The caller handles it or propagates with
  `?`. There are no exceptions.
- Enums are closed and `match` is exhaustive. The compiler tells you when you
  miss a case.
- Packages are explicit. Imports stay qualified. Exported APIs use `pub`.
- Generics use explicit type arguments. The compiler does not guess.
- Methods use explicit receivers. Value and pointer receivers are distinct.
- Dereferencing `nil`, including through pointer field access, terminates with
  a deterministic runtime error.
- Interfaces are named and implicit. A concrete type satisfies an interface by
  providing every required method with an exact signature match.
- Closures capture by value at creation time.
- Structured concurrency uses `taskgroup` for scoped spawning and `chan[T]`
  for bounded FIFO communication.
- The runtime owns heap allocation and reclaims unreachable managed storage
  with a conservative non-moving collector. There is no manual `free` or
  user-visible collection API.
- `YAR_GC_HEAP_TARGET_BYTES` overrides the initial 1 MiB collection threshold;
  it is primarily a runtime-testing and tuning control.
- The compiler produces LLVM IR and native executables through `clang`.
  There is no interpreter and no VM.
- The standard library is written in Yar and compiled through the same pipeline
  as user code.

## Types

`bool`, `i32`, `i64`, `str`, `error`, typed pointers (`*T`), structs,
interfaces, enums, fixed arrays (`[N]T`), slices (`[]T`), maps (`map[K]V`),
channels (`chan[T]`), and function types.

Fixed-array, slice, and string indexing is bounds-checked at runtime.
Signed integer addition, subtraction, multiplication, and negation wrap to the
operand width. Invalid division and remainder terminate deterministically.

## Concurrency

- `taskgroup []R { ... }` spawns concurrent calls and yields results in spawn
  order.
- `spawn` accepts named function calls and immediately called inline function
  literals inside a taskgroup body; arbitrary function values cannot be
  spawned. Builtins, methods, and host intrinsics without a dedicated task
  wrapper must be called from an inline literal instead.
- Spawn arguments and inline-literal captures must be recursively share-safe;
  pointers, slices, maps, interfaces, functions, and resource structs cannot
  cross the task boundary.
- `?` is rejected inside a taskgroup body because propagation could bypass the
  mandatory join; handle errors locally or propagate after the group yields.
- `chan[T]` is a bounded typed channel created with `chan_new[T](capacity)`.
- `chan_send`, `chan_recv`, and `chan_close` provide the channel operations.
- Each successful `spawn` starts one POSIX thread immediately.
- Windows builds compile, but concurrency operations currently fail at runtime
  with an unsupported message.

## Standard library

| Package   | What it does                                          |
| --------- | ----------------------------------------------------- |
| `strings` | Split, join, trim, contains, replace, case conversion |
| `utf8`    | Decoding and rune classification                      |
| `conv`    | Numeric and byte/string conversions                   |
| `sort`    | In-place sorting for slices                           |
| `path`    | Path normalization and joining                        |
| `fs`      | Text file, directory, and streaming file operations   |
| `io`      | Stream interfaces and copy/read helpers               |
| `process` | Argv access and child-process execution               |
| `env`     | Environment variable lookup                           |
| `stdio`   | Stderr output                                         |
| `net`     | TCP networking and stream wrappers                    |
| `http`    | Minimal HTTP/1.1 server helpers over TCP              |
| `testing` | Test assertions and framework                         |

See [`examples/http_server`](examples/http_server/) for a minimal native HTTP
service.

## Install

Requirements: Rust 2024 toolchain and `clang`.

```bash
cargo build --release -p yar-cli
./target/release/yar check main.yar
```

The Rust 2024 CLI is the shipped `yar` command:

```bash
./target/release/yar check main.yar
./target/release/yar emit-ir main.yar
./target/release/yar build main.yar
./target/release/yar run main.yar -- first-argument --flag
./target/release/yar test .
./target/release/yar init
./target/release/yar add local_lib --path=../local-lib
./target/release/yar add http https://github.com/user/http.git --tag=v1.0.0
./target/release/yar fetch
```

The CLI supports `check`, `emit-ir`, `build`, host `run`, host `test`, `init`,
and dependency manifest, lock, fetch, and update commands. It links the Rust
runtime static library for native build/run/test paths. `YAR_RUNTIME_BUNDLE`
may select a directory containing `yar-runtime.toml` and its static archive;
the CLI validates the bundle's target triple, runtime ABI, compiler
compatibility, archive path, and ordered system libraries before invoking
`clang`. Release installations discover `runtimes/<target-triple>/` next to the
`yar` executable, while source-tree host builds use the same checked-in bundle
manifest with a Cargo-built archive. Cross builds require a matching explicit
or installed bundle. `YAR_RUNTIME_ARCHIVE` is rejected with migration guidance.

A bundle is one target directory:

```text
yar-runtime.toml
libyar_runtime.a
```

The v1 manifest declares `format`, `target`, `runtime_abi`,
`compiler_compatibility`, `archive`, and `[link].system_libraries`. Library
names are data, not raw clang arguments; the CLI preserves their declared order.

Override the C compiler if needed:

```bash
CC=clang-17 ./bin/yar build main.yar
```

External build, test, and Git subprocesses have configurable operation
deadlines. The defaults are 30 seconds for native builds, 30 seconds for test
binaries, and 300 seconds for one dependency command:

```bash
YAR_BUILD_TIMEOUT_SECS=60 yar build main.yar
YAR_TEST_TIMEOUT_SECS=60 yar test .
YAR_GIT_TIMEOUT_SECS=600 yar lock
```

Values must be positive integer seconds. A `yar run` program itself has no
deadline; only its build phase uses `YAR_BUILD_TIMEOUT_SECS`. Timed subprocesses
are contained as a Unix process group or Windows Job Object so a timeout also
terminates ordinary descendants. A Unix descendant that deliberately creates a
new session is outside that containment boundary.

<details>
<summary>Installing clang</summary>

| Platform      | Command                                                                      |
| ------------- | ---------------------------------------------------------------------------- |
| macOS         | Included with Xcode Command Line Tools                                       |
| Debian/Ubuntu | `apt install clang`                                                          |
| Fedora        | `dnf install clang`                                                          |
| Windows       | `winget install LLVM.LLVM` or [releases.llvm.org](https://releases.llvm.org) |

</details>

## Commands

```text
yar <command> [arguments]
yar --manifest-path <path/to/yar.toml> <command> [arguments]
yar --help
yar --version
```

| Command   | What it does                                      |
| --------- | ------------------------------------------------- |
| `check`   | Parse and type-check without generating LLVM IR or a binary |
| `emit-ir` | Print LLVM IR to stdout                           |
| `build`   | Compile to a native executable                    |
| `run`     | Compile and execute a temporary native executable |
| `test`    | Discover and run test functions from `_test.yar`  |
| `init`    | Create a `yar.toml` manifest                      |
| `add`     | Add a dependency to `yar.toml`                    |
| `remove`  | Remove a dependency from `yar.toml`               |
| `fetch`   | Download dependencies from `yar.lock` to cache    |
| `lock`    | Regenerate `yar.lock` from `yar.toml`             |
| `update`  | Re-resolve dependencies and update `yar.lock`     |

Use `yar <command> --help` for command-specific usage. The form
`yar run <path> -- <arguments...>` forwards every argument after the required
`--` delimiter unchanged. The executed program remains responsible for its own
lifetime, and its numeric exit status becomes the `yar run` exit status.

## Testing

Test files end in `_test.yar`. `yar test` includes them only from the selected
entry package; imported packages and dependencies are compiled without their
test files. Every entry test-file function whose name starts with `test_` must
take exactly one `*testing.T` parameter, return non-errorable `void`, and have
no receiver or type parameters. Malformed declarations are diagnostics.

```
package main

import "std/testing"

fn add(a i32, b i32) i32 {
    return a + b
}

fn test_add(t *testing.T) void {
    testing.equal[i32](t, add(2, 3), 5)
    testing.equal[i32](t, add(-1, 1), 0)
}
```

```
$ yar test .
PASS: test_add

1 passed, 0 failed
```

## Dependencies

Yar uses git-based dependency management with no central registry.

The optional `--manifest-path` prefix appears at most once before the command,
must name `yar.toml`, and explicitly selects a project without fallback. Relative
manifest paths are resolved from the invocation directory. For `check`,
`emit-ir`, `build`, `run`, and `test`, Yar otherwise selects the nearest
ancestor manifest from the named entry file or directory; a manifestless entry
uses its own directory as the project root. An explicitly selected root must
contain the entry.

Dependency commands discover from the invocation directory. `add`, `remove`,
`fetch`, `lock`, and `update` use the nearest ancestor manifest. If none exists,
`add` creates one in the invocation directory while the other commands fail.
`init` does not search ancestors: it creates a project in the invocation
directory. Explicit `init` and `add` may create an absent `yar.toml` in an
existing selected directory; every other explicitly selected command requires
the manifest to exist. A created manifest derives its package name from the
selected directory when that name is valid and otherwise uses `myproject`.
Invalid nearest candidates fail rather than falling back to a parent project.

The selected manifest's directory owns its sibling `yar.lock`, recovery state,
root package tree, and manifest-relative local dependency paths. Relative
manifest, source, and output arguments still use the invocation directory, and
`run` or `test` programs keep that working directory.

```bash
yar init                                        # create yar.toml
yar add http https://github.com/user/http.git --tag=v1.0.0
yar build .
```

Dependencies are declared as aliases in `yar.toml`:

```toml
[package]
name = "myapp"

[dependencies]
http = { git = "https://github.com/user/yar-http.git", tag = "v0.3.1" }
local_lib = { path = "../my-local-lib" }
```

The alias becomes the first import-path segment: `import "http"`. The `std`
segment is reserved for compiler-owned packages such as `std/http`; dependency
aliases cannot use it. `std/...` imports resolve only to the embedded standard
library. Other imports resolve within the importing source's origin, checking
same-origin packages before aliases declared directly by that origin.

Compiler package identity is `PackageId = (source origin, source-relative
subpath)`, not the import text. Equal logical paths from different origins
therefore remain distinct, and lowered symbols include origin-safe identity.
Distinct imports in one package cannot use the same final path segment as their
source qualifier.

`yar.lock` is an explicit `version = 1` graph. It pins exact commit SHAs and
content hashes and records each package's full alias/git/ref child edges.
Commit it to version control. Before compilation reads dependency caches, and
before `yar fetch` uses the cache or network, Yar requires the manifest roots
and lock graph to match exactly. Duplicate aliases or edges, missing nodes,
cycles, source/ref conflicts, and unreachable entries are rejected. Old or
unversioned lock files must be regenerated with `yar lock`; review that diff
because lock/update resolution can select a new commit for a moved tag or
branch. `yar fetch` retrieves an existing lock by its commit SHAs and does not
re-resolve those mutable refs. Valid cache entries are verified offline; a
missing SHA that the remote cannot provide fails without falling back to its
recorded ref. Run `yar lock` or `yar update` to make that version change
explicit.

Dependency metadata changes are published recoverably. `yar add` and
`yar remove` resolve the complete target graph before changing either file,
then commit the new `yar.toml` together with the new `yar.lock` or its deletion.
`yar lock` and `yar update` preserve the manifest bytes. Pre-commit failures and
prepared interrupted transactions restore the prior pair, and success output
appears only after commit and journal cleanup. Existing metadata-file
permissions are preserved. Verified dependency caches may be warmed during
resolution and are not rolled back with project metadata. Explicit selection
recovers only its fixed manifest directory and never falls back. Automatic
discovery also recognizes transaction state when the live manifest is absent;
it recovers that candidate and restarts its ancestor search before reading
metadata. Do not run another Yar CLI command against the same selected project
while dependency metadata publication or recovery is in progress, even from a
different invocation directory.

The compiler hash-verifies a selected cache tree before reading its manifest or
source, then verifies the manifest against the recorded child edges. A missing,
modified, or edge-divergent selected entry fails closed instead of falling back
to a same-named standard-library package. Unused dependency caches remain lazy.

Lock reachability does not grant import visibility: each importing origin must
declare the external aliases it uses. Existing source that imported a merely
reachable transitive alias must add a direct declaration to its own manifest.
Lock v1 still requires each alias to identify one source/ref tuple across the
complete graph, so owner-local reuse of the same alias for different targets
requires a future lock v2. There is no root override for conflicting
source/ref selections.

Local path dependencies remain live and unhashed and may be declared only in
the root manifest. Relative values are resolved from the manifest directory,
not the invocation directory. Their manifests may contribute git dependencies,
but may not declare another path dependency. A selected path alias must exist
and does not fall through to a same-named standard-library package. `yar update
<git-alias>` merges the replacement graph, preserves unrelated nodes needed by
other roots, refreshes compatible shared nodes, and prunes orphans; targeted
path updates require `yar lock`.

## Cross-compilation

Set `YAR_OS` and `YAR_ARCH` to build for a different platform:

```bash
YAR_OS=linux YAR_ARCH=amd64 yar build main.yar
YAR_OS=windows YAR_ARCH=amd64 yar build main.yar -o main.exe
```

Supported targets:

| `YAR_OS`  | `YAR_ARCH` |
| --------- | ---------- |
| `darwin`  | `amd64`    |
| `darwin`  | `arm64`    |
| `linux`   | `amd64`    |
| `linux`   | `arm64`    |
| `windows` | `amd64`    |

Cross-compilation requires a `clang` that can target the requested platform
(appropriate sysroot and system libraries) and a matching runtime bundle. For
example, `YAR_RUNTIME_BUNDLE=/opt/yar-runtime` selects one validated bundle.
`yar run` and `yar test` only support the host platform.
The Windows target and release compiler use `x86_64-pc-windows-gnu`. Host ABIs
outside the exact supported little-endian Darwin and GNU triples are rejected
rather than treated as bundle-compatible.

## Documentation

- [The Yar Code](docs/language/the-yar-code.md) -- how to write Yar programs
- [Language reference](docs/YAR.md) -- what the compiler implements today
- [Language design docs](docs/language/) -- proposals, decisions, and process
- [Context docs](docs/context/) -- architecture, runtime, and compiler internals

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/verify-rust-testdata.sh
./scripts/verify-rust-testdata-run.sh
```

CI runs those gates on pull requests and pushes to `main`, with Linux and macOS
coverage for the native `clang` build boundary, the Rust workspace, and Rust
CLI native builds plus successful fixture execution for every checked-in
`testdata/**/main.yar` fixture that is expected to exit successfully. A targeted
Windows job exercises the subprocess Job Object lifecycle. Release packaging is
validated with a GoReleaser snapshot dry run; each artifact carries exactly one
target-keyed runtime bundle.

Version tags matching `v*` publish GitHub Release assets through GoReleaser.
Manual release workflow runs are snapshot-only and do not publish.

```text
crates/
  yar-cli/        Rust 2024 CLI entry point
  yar-compiler/   Rust 2024 compiler crate
  yar-process-control/ Shared subprocess deadlines and containment
  yar-runtime/    Rust 2024 runtime crate
stdlib/           Embedded standard library (Yar source)
testdata/         Representative sample programs
docs/             Language and design documentation
```

## License

[MIT](LICENSE)
