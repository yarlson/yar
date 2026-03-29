# Yar

Yar is a compiled programming language. Errors are visible. Variants are
closed. Output is native.

Read [The Yar Code](docs/language/the-yar-code.md) before you write a line.

## Quick look

```
package main

import "strings"

fn greet(name str) !void {
    if strings.contains(name, " ") {
        return error.InvalidName
    }
    print("hello, ")
    print(name)
    print("\n")
}

fn main() !i32 {
    greet("world")?
    return 0
}
```

```bash
./bin/yar run greet.yar
hello, world
```

## What Yar does

- Functions that can fail return `!T`. The caller handles it or propagates it.
  There are no exceptions.
- Enums are closed and `match` is exhaustive. The compiler tells you when you
  miss a case.
- Packages are explicit. Imports stay qualified. Exported APIs use `pub`.
- Generics use explicit type arguments. The compiler does not guess.
- Methods use explicit receivers. Value and pointer receivers are distinct.
- Interfaces are named and implicit. Concrete values satisfy them by matching
  the required methods exactly.
- Closures capture by value at creation time. No surprises.
- The runtime manages memory automatically. There is no manual `free` and no
  visible garbage collector — the runtime reclaims unreachable heap storage on
  its own.
- The compiler produces LLVM IR and native executables through `clang`.
  There is no interpreter and no VM.
- The standard library is written in Yar and compiled through the same pipeline
  as user code.

## Standard library

| Package   | What it does                            |
| --------- | --------------------------------------- |
| `strings` | String helpers and `parse_i64`          |
| `utf8`    | Decoding and rune classification        |
| `conv`    | Numeric and byte/string conversions     |
| `sort`    | In-place sorting for slices             |
| `path`    | Path normalization and joining          |
| `fs`      | Text file and directory operations      |
| `process` | Argv access and child-process execution |
| `env`     | Environment variable lookup             |
| `stdio`   | Stderr output                           |

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
yar <command> <path> [-o output]
```

| Command   | What it does                                      |
| --------- | ------------------------------------------------- |
| `check`   | Parse and type-check without generating a binary  |
| `emit-ir` | Print LLVM IR to stdout                           |
| `build`   | Compile to a native executable                    |
| `run`     | Compile and execute a temporary native executable |

## Documentation

- [The Yar Code](docs/language/the-yar-code.md) — how to write Yar programs
- [Language reference](docs/YAR.md) — what the compiler implements today
- [Language design docs](docs/language/) — proposals, decisions, and process
- [Context docs](docs/context/) — architecture, runtime, and compiler internals

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
  runtime/        Embedded C runtime
  stdlib/         Embedded standard library (Yar source)
testdata/         Representative sample programs
docs/             Language and design documentation
```

## License

[MIT](LICENSE)
