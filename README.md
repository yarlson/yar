# yar

`yar` is a small experimental language with:

- a Go frontend
- an LLVM IR backend
- a tiny C runtime for builtins

Current scope is intentionally small: single-file `package main` programs, top-level functions, `i32`/`i64`/`bool`/`str`/`void`/`noreturn`/`error`, `:=` declarations, assignment, `if`, `return`, function calls, explicit `!T` errorable returns, plain `error` values, `?` propagation sugar, `or |err| { ... }` local error handling, and `error.Name`.

## Status

This is v0 work. The compiler can already:

- parse and type-check a small language slice
- emit LLVM IR text
- link an executable with `clang`
- run the produced binary
- lower `?` and `or |err| { ... }` into explicit error checks and control flow

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

### Run a v0.2 program

```bash
go run ./cmd/yar run testdata/structs_and_loops.yar
```

This exercises structs, fixed arrays, `for`, `else`, `break`, `continue`,
field assignment, indexing, `var`, unary `!`, `%`, and `len`.

## Example Program

```yar
package main

fn divide(a i32, b i32) !i32 {
    if b == 0 {
        return error.DivideByZero
    }
    return a / b
}

fn main() !i32 {
    x := divide(10, 2)?
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

## Error Handling

`yar` keeps errors as explicit values. There are no exceptions or hidden unwinding semantics.

- `!T` means a function returns either a `T` or an error code.
- `error` is a plain builtin type for named errors and handler bindings.
- `?` means "propagate this error from the current function".
- `or |err| { ... }` means "handle this error here".

Examples:

```yar
x := divide(10, 2)?

write_file(path, data)?

x := divide(10, 2) or |err| {
    print("divide failed\n")
    return 0
}
```

These forms are syntax sugar. The compiler lowers them into explicit temporaries, error checks, branches, and returns.

## v0.2 Surface

The current language supports:

- top-level `struct` and `fn` declarations
- fixed arrays such as `[3]User`
- `if` / `else` / `else if`
- `for cond { ... }` and `for init; cond; post { ... }`
- `break` and `continue`
- `var` declarations in addition to `:=`
- field access and field assignment
- indexing and index assignment
- unary `-`, unary `!`, and `%`
- `len(array)`

Still not implemented:

- imports
- methods
- slices
- `&&`
- `||`

## How Builtins Work

The current builtins are:

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
- `len([N]T) i32`

Generated LLVM IR calls two runtime functions:

- `yar_print`
- `yar_print_int`
- `yar_panic`

Those functions are implemented in the runtime C source.

## Project Layout

- [cmd/yar/main.go](cmd/yar/main.go): CLI entrypoint
- [internal/compiler/compiler.go](internal/compiler/compiler.go): compile, build, and run orchestration
- [internal/lexer/lexer.go](internal/lexer/lexer.go): tokenization
- [internal/parser/parser.go](internal/parser/parser.go): parser
- [internal/checker/checker.go](internal/checker/checker.go): semantic analysis and type checking
- [internal/codegen/llvm.go](internal/codegen/llvm.go): LLVM IR generation
- [internal/runtime/runtime_source.txt](internal/runtime/runtime_source.txt): tiny runtime source used during linking
- [testdata/hello.yar](testdata/hello.yar): hello world example
- [testdata/add.yar](testdata/add.yar): arithmetic example
- [testdata/divide.yar](testdata/divide.yar): error propagation example
- [testdata/i64.yar](testdata/i64.yar): `i64` type-check and codegen example
- [testdata/unhandled_error.yar](testdata/unhandled_error.yar): unhandled error wrapper example
- [testdata/panic.yar](testdata/panic.yar): panic runtime example
- [testdata/structs_and_loops.yar](testdata/structs_and_loops.yar): v0.2 structs, arrays, and control-flow example

## Verify The Repository

Run:

```bash
go test ./...
```

This covers compiler-level tests and executable output checks for the current MVP slice.
