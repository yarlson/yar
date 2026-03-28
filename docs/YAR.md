# YAR

This document tracks the language that the compiler implements today.

It is intentionally descriptive, not aspirational. If the compiler and this
document disagree, the compiler is the source of truth and this file should be
updated.

## Scope

- Multi-file packages
- Entry `package main` plus imported packages
- Top-level `struct` and `fn` declarations, with optional `pub`
- Native code generation through LLVM IR plus `clang`

## File Shape

A valid entry package:

- starts with `package main`
- may contain one or more `.yar` files in the same directory
- may contain zero or more `import "path"` declarations after the package clause
- contains zero or more top-level `struct` declarations
- contains zero or more top-level `fn` declarations
- must define `main`

Imported packages:

- live in subdirectories under the entry package root
- use `import "path"` with slash-separated package paths
- must declare a package name matching the final import path segment
- may expose top-level `struct` and `fn` declarations with `pub`

`main` must return either:

- `i32`
- `!i32`

## Comments

The lexer supports line comments:

```yar
// this is a comment
```

## Types

Implemented types:

- `bool`
- `i32`
- `i64`
- `str`
- `void`
- `noreturn`
- `error`
- user-defined `struct` types
- fixed-size array types such as `[4]i32` and `[3]User`
- slice types such as `[]i32` and `[]User`

### Error-Related Types

- `!T` means a function returns either a success value of type `T` or an error.
- `!void` is valid and represents an operation that either succeeds or returns an error.
- Plain `error` is also a valid type for non-`main` functions, parameters, locals, and fields.

Current restrictions:

- parameters cannot use `void`
- parameters cannot use `noreturn`
- functions cannot use `!noreturn`
- functions cannot use `!error`
- struct fields cannot use `void`
- struct fields cannot use `noreturn`
- array elements cannot use `void`
- array elements cannot use `noreturn`
- slice elements cannot use `void`
- slice elements cannot use `noreturn`
- recursive struct containment is rejected

## Declarations

Local declarations:

```yar
x := 1
msg := "hi"
var count i32 = 0
var user User
```

`let` is not part of the language surface.

Locals:

- are block-scoped
- may be reassigned after declaration
- cannot be redeclared in the same scope

## Structs

User-defined structs are declared at top level:

```yar
struct User {
    id i32
    name str
}
```

Supported struct operations:

- field access: `user.name`
- keyed literals: `User{id: 1, name: "bob"}`
- empty literals: `User{}`
- field assignment: `user.name = "eve"`

There are no:

- methods
- embedding
- tags

Top-level visibility uses `pub`:

```yar
pub struct User {
    id i32
}

pub fn lookup() User {
    return User{id: 1}
}
```

Exported declarations cannot expose package-local struct types through public
fields, parameters, or return types.

## Arrays

Fixed arrays are supported:

```yar
nums := [4]i32{1, 2, 3, 4}
first := nums[0]
nums[1] = 10
```

Supported array operations:

- array types: `[N]T`
- array literals
- indexing
- index assignment
- `len(array)`

## Slices

Slices are supported:

```yar
values := []i32{}
values = append(values, 1)
values = append(values, 2)
part := values[0:1]
part[0] = 9
```

Supported slice operations:

- slice types: `[]T`
- slice literals
- indexing
- index assignment
- slicing with `s[i:j]`
- `len(slice)`
- `append(slice, value)` returning an updated slice

Slices are views over runtime-managed backing storage. Slicing shares storage,
and `append` may reuse that storage or allocate a new backing buffer.

Slice indexing and slicing are bounds-checked at runtime and trap on invalid ranges.

## Functions

Functions are declared with `fn`:

```yar
fn add(a i32, b i32) i32 {
    return a + b
}
```

Parameters are positional and explicitly typed.

There are no:

- methods
- generics
- variadics

Cross-package function calls stay qualified:

```yar
package main

import "lexer"

fn main() i32 {
    return lexer.exit_code()
}
```

## Statements

Implemented statements:

- block statements: `{ ... }`
- short local declarations: `x := expr`
- typed local declarations: `var name Type` and `var name Type = expr`
- assignment to locals, struct fields, array indices, and slice indices
- `if`
- `if` / `else`
- `else if`
- `for cond { ... }`
- `for init; cond; post { ... }`
- `break`
- `continue`
- `return`
- expression statements

## Expressions

Implemented expressions:

- local identifiers
- package-qualified function calls
- integer literals
- string literals
- boolean literals
- struct literals
- array literals
- slice literals
- function calls
- parenthesized expressions
- field access
- indexing
- slicing
- unary `-`
- unary `!`
- short-circuit boolean operators: `&&`, `||`
- binary arithmetic: `+`, `-`, `*`, `/`, `%`
- binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`
- postfix error propagation: `expr?`
- local error handling: `expr or |err| { ... }`

### Integer Literals

Integer literals start as untyped integers and are coerced by context into `i32`
or `i64`.

### Strings

Supported escapes:

- `\n`
- `\t`
- `\\`
- `\"`

### Boolean Operators

- `a && b` evaluates `b` only when `a` is `true`
- `a || b` evaluates `b` only when `a` is `false`
- both operands must be non-errorable `bool`
- the result type is `bool`

## Error Model

Errors are explicit values. There are no exceptions, hidden stack unwinding, or
`try`/`catch` semantics.

Named errors are written as:

```yar
error.DivideByZero
```

Today, `error.Name` is only valid as the direct operand of `return` inside:

- an errorable function returning `!T`
- a function returning plain `error`

The checker records every distinct returned error name and assigns it a numeric
code for code generation.

## Error Handling Forms

### Direct Return

An errorable call may be returned directly from a function with the same
errorable result type:

```yar
fn fail() !i32 {
    return error.Boom
}

fn main() !i32 {
    return fail()
}
```

### `?`

`?` is propagation sugar.

It is valid on:

- `!T`
- `error`

Examples:

```yar
x := divide(10, 2)?
user := lookup(1)?
write_file(path, data)?
```

Meaning:

- if the expression succeeds, continue
- if it carries an error, return that error from the current function

Current checker rule:

- `?` may only be used inside a function that can return an error, meaning a
  function returning `!T` or plain `error`

### `or |err| { ... }`

`or |err| { ... }` is local handling sugar.

Examples:

```yar
x := divide(10, 2) or |err| {
    return 0
}

user := lookup(1) or |err| {
    return User{}
}
```

Handler rules:

- valid on `!T`
- valid on `error`
- the bound name has type `error`
- the bound name exists only inside the handler block
- the handler runs only when the error is non-nil

Additional rule for value-producing `!T`:

- if `or` is used where success produces a value, the handler block must
  terminate control flow

## Raw Errorable Values

Raw errorable expressions cannot be used directly in:

- `:=` declarations
- `var` initializers
- assignments
- function arguments
- unary operators
- binary operators
- field access
- indexing
- conditions
- non-propagating returns
- plain expression statements

They must be handled immediately with one of:

- direct `return`
- `?`
- `or |err| { ... }`

## Return Rules

- `void` functions may use bare `return`
- `!void` functions may use bare `return` for successful completion
- non-`void` functions must return a value on all reachable paths
- `noreturn` functions cannot contain `return`

## Builtins

Builtins are fixed by the compiler:

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
- `len([N]T | []T) i32`
- `append([]T, T) []T`

They are not user-overridable.

## Runtime Behavior

When `main` returns `!i32`:

- success returns the `i32` value to the process
- error prints `unhandled error: <Name>` and exits with status `1`

If the runtime sees an unknown error code, it prints `unhandled error` and exits
with status `1`.

`panic(str)` writes to stderr and exits with status `1`.

Out-of-range slice indexing and invalid slice ranges trap with a runtime failure.

## Not Implemented

The compiler does not currently implement:

- methods
- enums
- import aliases
- pattern matching
- exceptions
- automatic recovery
