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

import "strings"

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
- The runtime owns heap allocation. The current Rust runtime retains allocated
  storage until process exit; there is no manual `free` or user-visible
  collector.
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
./target/release/yar run main.yar
./target/release/yar test .
./target/release/yar init
./target/release/yar add local_lib --path=../local-lib
./target/release/yar add http https://github.com/user/http.git --tag=v1.0.0
./target/release/yar fetch
```

The CLI supports `check`, `emit-ir`, `build`, host `run`, host `test`, `init`,
and dependency manifest, lock, fetch, and update commands. It links the Rust
runtime static library for native build/run/test paths, using
`YAR_RUNTIME_ARCHIVE` when set, a runtime archive next to the `yar` executable
when present, or the workspace `target/release` archive after building
`crates/yar-runtime`. Cross builds require
`YAR_RUNTIME_ARCHIVE` to point at a runtime archive for the selected target.
GoReleaser release artifacts now package the Rust CLI with a sibling Rust
runtime archive. The legacy embedded C runtime has been removed; native builds
use the Rust runtime only.

Override the C compiler if needed:

```bash
CC=clang-17 ./bin/yar build main.yar
```

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
```

| Command   | What it does                                      |
| --------- | ------------------------------------------------- |
| `check`   | Parse and type-check without generating a binary  |
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

## Testing

Test files end in `_test.yar`. Test functions take `*testing.T` and return `void`:

```
package main

import "testing"

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

2 passed, 0 failed
```

## Dependencies

Yar uses git-based dependency management with no central registry.

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

The alias becomes the import path: `import "http"`. Resolution order: local >
dependency > stdlib.

`yar.lock` pins exact commit SHAs and content hashes. Commit it to version
control. The compiler verifies locked cache contents before loading dependency
source, and `yar fetch` verifies both existing and newly downloaded entries.
When selected, a missing or modified locked cache entry fails closed instead
of falling back to a same-named standard-library package.

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
(appropriate sysroot and system libraries). `yar run` and `yar test` only
support the host platform.

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
`testdata/**/main.yar` fixture that is expected to exit successfully.
Release packaging is validated with a GoReleaser snapshot dry run.

Version tags matching `v*` publish GitHub Release assets through GoReleaser.
Manual release workflow runs are snapshot-only and do not publish.

```text
crates/
  yar-cli/        Rust 2024 CLI entry point
  yar-compiler/   Rust 2024 compiler crate
  yar-runtime/    Rust 2024 runtime crate
stdlib/           Embedded standard library (Yar source)
testdata/         Representative sample programs
docs/             Language and design documentation
```

## License

[MIT](LICENSE)
