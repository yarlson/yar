# yar

`yar` is a small experimental language with:

- a Go frontend
- an LLVM IR backend
- a tiny C runtime for builtins

Current scope includes multi-package programs with `import` and `pub` exports, top-level `struct`, `enum`, and `fn` declarations, `i32`/`i64`/`bool`/`str`/`void`/`noreturn`/`error`, typed pointers, fixed arrays, slices, `:=` and `var` declarations, assignment, `if`/`else`, `for` loops with `break`/`continue`, exhaustive `match` over enums, function calls, explicit `!T` errorable returns, plain `error` values, `?` propagation sugar, `or |err| { ... }` local error handling, short-circuit `&&`/`||`, and `error.Name`.

## Status

This is v0 work. The compiler can already:

- parse and type-check multi-package programs
- emit LLVM IR text
- link an executable with `clang`
- run the produced binary
- lower `?` and `or |err| { ... }` into explicit error checks and control flow
- compile structs, enums with payload cases, fixed arrays, slices, and typed pointers
- lower exhaustive `match` over enums into tagged-union dispatch
- resolve multi-package imports with `pub` export validation

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
go run ./cmd/yar check testdata/hello/main.yar
```

If the program is valid, this prints nothing and exits with code `0`.

### Emit LLVM IR

```bash
go run ./cmd/yar emit-ir testdata/hello/main.yar > hello.ll
```

This writes textual LLVM IR to `hello.ll`.

### Build an executable

```bash
go run ./cmd/yar build testdata/hello/main.yar -o hello
```

This does all of the following:

1. parses and type-checks the `.yar` file
2. emits LLVM IR
3. materializes the tiny runtime C source
4. invokes `clang`
5. produces a native executable at `./hello`

The flag order also works like this:

```bash
go run ./cmd/yar build -o hello testdata/hello/main.yar
```

### Run a program directly

```bash
go run ./cmd/yar run testdata/hello/main.yar
```

This builds to a temporary executable and runs it immediately.

### Run a multi-package program

```bash
go run ./cmd/yar run testdata/imports_ok/
```

This exercises multi-package imports with `pub` exports and package-qualified calls.

### Run a structs and loops program

```bash
go run ./cmd/yar run testdata/structs_and_loops/main.yar
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
go run ./cmd/yar run testdata/divide/main.yar
```

## Manual Compile + Link Flow

If you want to see the real artifact boundary, `yar` can be used in two explicit steps.

### 1. Emit LLVM IR

```bash
go run ./cmd/yar emit-ir testdata/hello/main.yar > hello.ll
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

## Current Language Surface

The current language supports:

- multi-file packages with `import` and `pub` exports
- top-level `struct`, `enum`, and `fn` declarations
- user-defined enums with plain and payload-carrying cases
- exhaustive `match` over enum values with payload binding
- fixed arrays such as `[3]User`
- slices such as `[]i32` with `append`, `len`, and runtime bounds checks
- typed pointers with `&`, `*`, and `nil`
- `if` / `else` / `else if`
- `for cond { ... }` and `for init; cond; post { ... }`
- `break` and `continue`
- `var` declarations in addition to `:=`
- field access and field assignment
- indexing and index assignment
- unary `-`, unary `!`, `&`, `*`, and `%`
- short-circuit `&&` and `||`
- `len(array-or-slice)` and `append(slice, value)`

Still not implemented:

- methods
- generics
- closures or lambdas

## How Builtins Work

The current builtins are:

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
- `len([N]T | []T) i32`
- `append([]T, T) []T`

Generated LLVM IR calls runtime functions:

- `yar_print`
- `yar_print_int`
- `yar_panic`
- `yar_alloc` / `yar_alloc_zeroed`
- `yar_slice_index_check` / `yar_slice_range_check`

Those functions are implemented in the runtime C source.

## Project Layout

- [cmd/yar/main.go](cmd/yar/main.go): CLI entrypoint
- [internal/token/token.go](internal/token/token.go): token types and source positions
- [internal/diag/diag.go](internal/diag/diag.go): diagnostic reporting
- [internal/ast/ast.go](internal/ast/ast.go): AST node types and package graph structures
- [internal/compiler/compiler.go](internal/compiler/compiler.go): compile, build, and run orchestration
- [internal/compiler/packages.go](internal/compiler/packages.go): multi-package loading and lowering
- [internal/lexer/lexer.go](internal/lexer/lexer.go): tokenization
- [internal/parser/parser.go](internal/parser/parser.go): parser
- [internal/checker/checker.go](internal/checker/checker.go): semantic analysis and type checking
- [internal/codegen/llvm.go](internal/codegen/llvm.go): LLVM IR generation
- [internal/runtime/runtime_source.txt](internal/runtime/runtime_source.txt): tiny runtime source used during linking
- [testdata/hello/](testdata/hello/): hello world
- [testdata/add/](testdata/add/): arithmetic
- [testdata/divide/](testdata/divide/): error propagation with `?`
- [testdata/i64/](testdata/i64/): `i64` type-check and codegen
- [testdata/bool_operators/](testdata/bool_operators/): short-circuit `&&` and `||`
- [testdata/unhandled_error/](testdata/unhandled_error/): unhandled error wrapper
- [testdata/panic/](testdata/panic/): panic runtime behavior
- [testdata/structs_and_loops/](testdata/structs_and_loops/): structs, arrays, and control flow
- [testdata/slices/](testdata/slices/): slice operations with `append` and bounds checks
- [testdata/pointers/](testdata/pointers/): pointer dereference and address-of
- [testdata/enums/](testdata/enums/): enum definitions and exhaustive `match`
- [testdata/imports_ok/](testdata/imports_ok/): multi-package imports

## Verify The Repository

Run:

```bash
go test ./...
```

This covers compiler-level tests and executable output checks for the current MVP slice.
