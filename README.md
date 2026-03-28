# yar

A compiled language with explicit error handling, enums with payloads, and a multi-package module system. Yar compiles to native executables through LLVM IR and clang.

```yar
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
$ yar run greet.yar
hello, world
```

## What yar has

- Structs, enums with payloads, exhaustive `match`
- Typed pointers, fixed arrays, slices, maps
- Multi-file packages with `import` and `pub` exports
- Error handling: `!T` return types, `?` propagation, `or |err| { ... }`
- Standard library: `strings`, `utf8`, `conv`
- Compiles to native code (no interpreter, no VM)
- Zero external Go dependencies

## What yar does not have (yet)

Methods, generics, closures, interfaces, garbage collection. See the [language spec](docs/YAR.md) for current scope.

## Install

Requires **Go 1.26+** and **clang**.

```bash
go build -o ./bin/yar ./cmd/yar
```

<details>
<summary>Installing clang</summary>

| Platform      | Command                                                                      |
| ------------- | ---------------------------------------------------------------------------- |
| macOS         | Included with Xcode Command Line Tools                                       |
| Debian/Ubuntu | `apt install clang`                                                          |
| Fedora        | `dnf install clang`                                                          |
| Windows       | `winget install LLVM.LLVM` or [releases.llvm.org](https://releases.llvm.org) |

Set `CC` to use a specific version: `CC=clang-17 yar build main.yar`

</details>

## Commands

```
yar <command> <file> [-o output]
```

| Command   | Description                                       |
| --------- | ------------------------------------------------- |
| `check`   | Type-check without generating code                |
| `emit-ir` | Print LLVM IR to stdout                           |
| `build`   | Compile to a native executable (default: `a.out`) |
| `run`     | Compile and execute                               |

## Language tour

### Error handling

Functions that can fail return `!T`. Callers must handle the error — the compiler enforces this.

```yar
fn parse(input str) !i32 {
    if len(input) == 0 {
        return error.Empty
    }
    return 42
}

fn main() !i32 {
    // propagate with ?
    value := parse("test")?

    // or handle locally
    fallback := parse("") or |err| {
        print("using default\n")
        return 0
    }

    return 0
}
```

### Enums and match

Enums are closed variant types. Cases can carry payloads. `match` is exhaustive.

```yar
enum Expr {
    Int { value i32 }
    Name { text str }
}

fn eval(e Expr) i32 {
    match e {
    case Expr.Int(v) {
        return v.value
    }
    case Expr.Name(v) {
        return len(v.text)
    }
    }
}
```

### Packages

Packages are directories of `.yar` files. Exported declarations use `pub`.

```yar
// lexer/lexer.yar
package lexer

pub fn classify(ch i32) str {
    if ch >= 48 {
        if ch <= 57 {
            return "digit"
        }
    }
    return "other"
}
```

```yar
// main.yar
package main

import "lexer"

fn main() i32 {
    kind := lexer.classify(65)
    print(kind)
    print("\n")
    return 0
}
```

### Pointers and recursive data

```yar
struct Node {
    value i32
    next *Node
}

fn main() i32 {
    tail := &Node{value: 2, next: nil}
    head := &Node{value: 1, next: tail}
    print_int((*head).value)
    print("\n")
    return 0
}
```

### Maps

Map indexing returns `!V` — missing keys are errors, not silent zero values.

```yar
fn main() !i32 {
    m := map[str]i32{"x": 1, "y": 2}
    v := m["x"]?
    print_int(v)
    print("\n")

    m["z"] = 3
    delete(m, "x")
    return 0
}
```

## Development

```bash
# Run tests
go test -race -count=1 -v -timeout=120s ./...

# Lint
golangci-lint run --fix ./...
```

### Project structure

```
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
testdata/         Test programs
docs/             Language spec and design docs
```

## Docs

- [Language specification](docs/YAR.md) — what the compiler implements today
- [Language design](docs/language/) — proposals, decisions, roadmap

## License

[MIT](LICENSE)
