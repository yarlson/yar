# yar

`yar` is a small experimental language with:

- a Go frontend
- an LLVM IR backend
- a tiny C runtime for builtins

Current scope is intentionally small: single-file `package main` programs, top-level functions, `i32`/`bool`/`str`/`void`, `let`, assignment, `if`, `return`, function calls, explicit `!T` errorable returns, `error.Name`, and `catch`.

## Status

This is v0 work. The compiler can already:

- parse and type-check a small language slice
- emit LLVM IR text
- link an executable with `clang`
- run the produced binary

## Requirements

You need:

- Go 1.26+
- `clang`

Tested locally with:

- `go version go1.26.1`
- Apple clang 21 on macOS arm64

`yar` emits LLVM IR text and relies on `clang` to compile and link it.

## Build The Compiler

Run the compiler directly with Go:

```bash
go run ./cmd/yar <command> <file>
```

Or build a standalone compiler binary:

```bash
go build -o ./bin/yar ./cmd/yar
```

After that, use:

```bash
./bin/yar <command> <file>
```

## Commands

### Type-check a program

```bash
go run ./cmd/yar check testdata/hello.yar
```

If the program is valid, this prints nothing and exits with code `0`.

### Emit LLVM IR

```bash
go run ./cmd/yar emit-ir testdata/hello.yar > hello.ll
```

This writes textual LLVM IR to `hello.ll`.

### Build an executable

```bash
go run ./cmd/yar build testdata/hello.yar -o hello
```

This does all of the following:

1. parses and type-checks the `.yar` file
2. emits LLVM IR
3. materializes the tiny runtime C source
4. invokes `clang`
5. produces a native executable at `./hello`

The flag order also works like this:

```bash
go run ./cmd/yar build -o hello testdata/hello.yar
```

### Run a program directly

```bash
go run ./cmd/yar run testdata/hello.yar
```

This builds to a temporary executable and runs it immediately.

## Example Program

```yar
package main

fn divide(a i32, b i32) !i32 {
    if b == 0 {
        return error.DivByZero
    }
    return a / b
}

fn main() i32 {
    let x = divide(10, 2) catch {
        print("division failed\n")
        return 1
    }

    print_int(x)
    print("\n")
    return 0
}
```

Build and run it:

```bash
go run ./cmd/yar run testdata/divide.yar
```

## Manual Compile + Link Flow

If you want to see the real artifact boundary, `yar` can be used in two explicit steps.

### 1. Emit LLVM IR

```bash
go run ./cmd/yar emit-ir testdata/hello.yar > hello.ll
```

### 2. Link the IR with the runtime using `clang`

The runtime source lives in [internal/runtime/runtime_source.txt](internal/runtime/runtime_source.txt). It is stored as text so Go can embed it without requiring cgo.

Copy it to a `.c` file and link it with the emitted IR:

```bash
cp internal/runtime/runtime_source.txt runtime.c
clang -Wno-override-module hello.ll runtime.c -o hello
./hello
```

That is effectively what `yar build` automates.

## How Builtins Work

The current builtins are:

- `print(str) void`
- `print_int(i32) void`

Generated LLVM IR calls two runtime functions:

- `yar_print`
- `yar_print_int`

Those functions are implemented in the runtime C source.

## Project Layout

- [cmd/yar/main.go](cmd/yar/main.go): CLI entrypoint
- [internal/compiler/compiler.go](internal/compiler/compiler.go): compile, build, and run orchestration
- [internal/parser/parser.go](internal/parser/parser.go): parser
- [internal/checker/checker.go](internal/checker/checker.go): semantic analysis and type checking
- [internal/codegen/llvm.go](internal/codegen/llvm.go): LLVM IR generation
- [internal/runtime/runtime_source.txt](internal/runtime/runtime_source.txt): tiny runtime source used during linking
- [testdata/hello.yar](testdata/hello.yar): hello world example
- [testdata/add.yar](testdata/add.yar): arithmetic example
- [testdata/divide.yar](testdata/divide.yar): error handling example

## Verify The Repository

Run:

```bash
go test ./...
```

This covers compiler-level tests and executable output checks for the current MVP slice.
