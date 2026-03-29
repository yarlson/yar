# yar

`yar` takes a hard line on program shape: errors are explicit, variants are
closed, and output is native. It is for people who want a language that makes
failure handling visible, keeps data modeling strict, and compiles all the way
down to executables through LLVM IR and `clang`.

## Why yar

- Errors are part of the function contract, not a side channel.
- Enums are closed and `match` is exhaustive, so branching stays honest.
- Packages are explicit, exported APIs are deliberate, and cross-package access
  stays visible in the source.
- The compiler produces LLVM IR and native executables. There is no interpreter
  and no VM boundary.
- The standard library is written in yar and compiled through the same pipeline
  as user code.

## Quick Example

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

## What it already does

Yar currently supports:

- Top-level `struct`, `enum`, `fn`, and method declarations
- Explicit generic `struct` and `fn` declarations with explicit type arguments
- `bool`, `i32`, `i64`, `str`, `void`, `noreturn`, `error`
- Typed pointers, fixed arrays, slices, maps, and function values
- Multi-file packages rooted at an entry `package main`
- `if`, `for`, `break`, `continue`, `return`, and exhaustive `match`
- Methods on named struct types with explicit value or pointer receivers
- Anonymous function literals and lexical capture-by-value closures
- String indexing, slicing, concatenation, and equality
- Native builds, IR emission, and direct execution from the CLI

Current method support is intentionally small and explicit:

- Methods are declared as `fn (u User) label() str { ... }`
- Calls use `value.method(...)`
- Receiver matching is exact; yar does not insert implicit `&` or `*`

Current generics support is intentionally small and explicit:

- Generic declarations look like `struct Box[T] { ... }` and `fn first[T](...) T`
- Use sites must supply type arguments such as `Box[i32]` and `first[i32](values)`
- Instantiated generic code is monomorphized before type-checking and code generation
- There is no type-argument inference, no constraints, and no generic methods

Current closure support is intentionally small and explicit:

- Function types are written as `fn(T1, T2) R` or `fn(T) !R`
- Anonymous function literals use `fn(...) R { ... }`
- Closures capture outer locals by value at creation time
- Captured outer locals are readable inside closure bodies but not assignable there
- Method values are still not first-class; `value.method` must be called immediately

The embedded standard library currently includes:

- `strings` — string helpers and `parse_i64`
- `utf8` — decoding and rune classification
- `conv` — numeric and byte/string conversion helpers
- `sort` — in-place sorting for `[]str`, `[]i32`, and `[]i64`
- `path` — path normalization and joining
- `fs` — text file and directory operations
- `process` — argv access and child-process execution
- `env` — environment lookup
- `stdio` — stderr output

## What it does not try to do

Yar does not currently have:

- Type-argument inference or generic constraints
- Interfaces
- Garbage collection

The language and standard library are intentionally constrained. The compiler
is the source of truth for implemented behavior.

## Install

Requirements:

- Go 1.26+
- `clang`

Build the CLI:

```bash
go build -o ./bin/yar ./cmd/yar
```

Use `CC` to override the compiler command if needed:

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

## Why this repo matters

Use yar when you want:

- A language and compiler you can actually read end to end
- A language with explicit failures instead of implicit exception flow
- Native executables without a VM boundary
- A current implementation that already covers packages, enums, maps,
  pointers, closures, and host-backed stdlib calls

The implemented surface is intentionally explicit. See
[docs/YAR.md](docs/YAR.md) for the exact behavior the compiler supports today.

## Development

Run tests:

```bash
go test -race -count=1 -v -timeout=120s ./...
```

Run lint:

```bash
golangci-lint run --fix ./...
```

Repository layout:

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
  stdlib/         Embedded standard library (yar source)
testdata/         Representative sample programs
docs/             Language and design documentation
```

## Documentation

- [Language reference](docs/YAR.md) — what the compiler implements today
- [Language design docs](docs/language/) — proposals, decisions, and process
- [Context docs](docs/context/) — current architecture, runtime, and compiler behavior

## License

[MIT](LICENSE)
