# Yar

<p align="center">
  <img src="assets/yar-banner.jpg" alt="Yar — The Yar Code" width="600">
</p>

Yar is a compiled programming language that targets native executables through
LLVM. It has explicit error handling, closed enums with exhaustive matching,
generics, interfaces, structured concurrency, and automatic memory management
with no exceptions, no implicit coercions, and no hidden control flow.

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
- Interfaces are named and implicit. A concrete type satisfies an interface by
  providing every required method with an exact signature match.
- Closures capture by value at creation time.
- Structured concurrency uses `taskgroup` for scoped spawning and `chan[T]`
  for bounded FIFO communication.
- The runtime manages memory automatically. There is no manual `free` and no
  visible garbage collector.
- The compiler produces LLVM IR and native executables through `clang`.
  There is no interpreter and no VM.
- The standard library is written in Yar and compiled through the same pipeline
  as user code.

## Types

`bool`, `i32`, `i64`, `str`, `error`, typed pointers (`*T`), structs,
interfaces, enums, fixed arrays (`[N]T`), slices (`[]T`), maps (`map[K]V`),
channels (`chan[T]`), and function types.

## Concurrency

- `taskgroup []R { ... }` spawns concurrent calls and yields results in spawn
  order.
- `spawn call(...)` is valid only inside a taskgroup body.
- `chan[T]` is a bounded typed channel created with `chan_new[T](capacity)`.
- `chan_send`, `chan_recv`, and `chan_close` provide the channel operations.
- The current implementation uses POSIX threads under the hood.
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
| `fs`      | Text file and directory operations                    |
| `process` | Argv access and child-process execution               |
| `env`     | Environment variable lookup                           |
| `stdio`   | Stderr output                                         |
| `net`     | TCP networking (listen, connect, read, write)         |
| `testing` | Test assertions and framework                         |

## Install

Requirements: Go 1.26+ and `clang`.

```bash
go build -o ./bin/yar ./cmd/yar
```

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
control for reproducible builds.

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
go test -race -count=1 -v -timeout=120s ./...
golangci-lint run --fix ./...
```

```text
cmd/yar/          CLI entry point
internal/
  lexer/          Tokenizer
  parser/         Syntax analysis
  ast/            AST node types
  checker/        Type checking and semantic analysis
  codegen/        LLVM IR generation
  compiler/       Pipeline orchestration and package loading
  deps/           Dependency management (yar.toml, yar.lock, fetching)
  runtime/        Embedded C runtime
  stdlib/         Embedded standard library (Yar source)
testdata/         Representative sample programs
docs/             Language and design documentation
```

## License

[MIT](LICENSE)
