# yar

A compiler for the yar programming language, written in Go. Yar generates native executables through LLVM IR and clang.

- **Multi-file packages** with imports and `pub` exports
- **Structs, enums, fixed arrays, slices, maps** with full type checking
- **Explicit error handling** via `!T` return types, `?` propagation, and `or |err| { ... }`
- **Exhaustive match expressions** for enum variants
- **Standard library** with `strings`, `utf8`, and `conv` packages
- **Zero external Go dependencies** — pure standard library implementation

> **Note**: This is v0 work. Methods, generics, and closures are not yet implemented.

## Prerequisites

- **Go 1.26.0** or later
- **clang** (for linking LLVM IR to native executables)

### Installing clang

| Platform              | Command                                                                                    |
| --------------------- | ------------------------------------------------------------------------------------------ |
| macOS                 | Xcode Command Line Tools (ships with clang)                                                |
| Linux (Debian/Ubuntu) | `apt install clang`                                                                        |
| Linux (Fedora)        | `dnf install clang`                                                                        |
| Windows               | Download from [releases.llvm.org](https://releases.llvm.org) or `winget install LLVM.LLVM` |

## Install

```bash
go build -o ./bin/yar ./cmd/yar
```

Or run directly without building:

```bash
go run ./cmd/yar <command> <file>
```

## Quickstart

Create a file `hello.yar`:

```yar
package main

fn main() i32 {
    print("hello, world\n")
    return 0
}
```

Build and run:

```bash
go run ./cmd/yar build hello.yar -o hello
./hello
```

Or compile and run in one step:

```bash
go run ./cmd/yar run hello.yar
```

## Usage

```
yar <check|emit-ir|build|run> <file> [-o output]
```

### Commands

| Command   | Description                                    |
| --------- | ---------------------------------------------- |
| `check`   | Type-check the program without generating code |
| `emit-ir` | Output LLVM IR to stdout                       |
| `build`   | Compile to a native executable                 |
| `run`     | Compile and execute immediately                |

### Examples

```bash
# Type-check a program
go run ./cmd/yar check myprogram.yar

# View generated LLVM IR
go run ./cmd/yar emit-ir myprogram.yar

# Build with custom output name
go run ./cmd/yar build myprogram.yar -o myprogram

# Compile and run
go run ./cmd/yar run myprogram.yar
```

## Configuration

| Variable | Description                                                 | Required | Default      |
| -------- | ----------------------------------------------------------- | -------- | ------------ |
| `CC`     | Path to clang binary or specific version (e.g., `clang-17`) | No       | System clang |

### Example

```bash
CC=clang-17 go run ./cmd/yar build testdata/hello/main.yar -o hello
```

## Troubleshooting

### Language is v0 experimental

This is early-stage work. Methods, generics, and closures are not yet implemented. See [docs/YAR.md](docs/YAR.md) for the current language specification.

### Requires external toolchain (clang)

Compilation depends on system clang for linking LLVM IR to native executables. Set the `CC` environment variable to use a specific version:

```bash
CC=clang-17 go run ./cmd/yar build myprogram.yar
```

### No package manager integration

The yar compiler is standalone with zero external Go dependencies. The system must provide clang separately.

## Development

### Run tests

```bash
go test ./...
```

With full flags:

```bash
go test -race -count=1 -v -timeout=120s ./...
```

### Lint

```bash
golangci-lint run --fix ./...
```

### Project structure

```
cmd/yar/           CLI entry point
internal/
  lexer/           Tokenizer
  parser/          Syntax analysis
  ast/             Abstract syntax tree
  checker/         Semantic analysis and type checking
  codegen/         LLVM IR generation
  compiler/        Compilation orchestration
  runtime/         C runtime source
  stdlib/          Embedded standard library
testdata/          Test programs covering language features
docs/              Language specification and design documents
```

## Contributing

Not documented. Check the repository for contribution guidelines.

## License

Not documented. Check the repository for license information.
