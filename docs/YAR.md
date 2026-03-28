# YAR

This document tracks the language that the compiler implements today.

It is intentionally descriptive, not aspirational. If the compiler and this
document disagree, the compiler is the source of truth and this file should be
updated.

## Scope

- Multi-file packages
- Entry `package main` plus imported packages
- Top-level `struct`, `enum`, and `fn` declarations, with optional `pub`
- Native code generation through LLVM IR plus `clang` (overridable via `CC` environment variable)

## File Shape

A valid entry package:

- starts with `package main`
- may contain one or more `.yar` files in the same directory
- may contain zero or more `import "path"` declarations after the package clause
- contains zero or more top-level `struct` declarations
- contains zero or more top-level `enum` declarations
- contains zero or more top-level `fn` declarations
- must define `main`

Imported packages:

- live in subdirectories under the entry package root
- use `import "path"` with slash-separated package paths
- must declare a package name matching the final import path segment
- may expose top-level `struct`, `enum`, and `fn` declarations with `pub`

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
- typed pointer types such as `*Node` and `*[4]i32`
- user-defined `struct` types
- user-defined `enum` types
- fixed-size array types such as `[4]i32` and `[3]User`
- slice types such as `[]i32` and `[]User`
- map types such as `map[str]i32` and `map[i32]bool`

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
- enum payload fields cannot use `void`
- enum payload fields cannot use `noreturn`
- pointer targets cannot use `void`
- pointer targets cannot use `noreturn`
- direct recursive struct or enum containment is rejected, but recursive shapes through pointers are valid

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

Recursive data is modeled through pointer indirection:

```yar
struct Node {
    value i32
    next *Node
}
```

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

Exported declarations cannot expose package-local types (struct or enum) through
public fields, parameters, or return types.

## Enums

Enums model closed sets of named variants:

```yar
enum TokenKind {
    Ident
    Int
}

enum Expr {
    Int { value i32 }
    Name { text str }
}
```

Supported enum operations:

- plain cases such as `TokenKind.Ident`
- payload constructors such as `Expr.Name{text: "main"}`
- exhaustive `match`
- payload binding inside `match` arms with `case Expr.Name(v) { ... }`
- payload ignore binding with `case Expr.Int(_) { ... }`

`match` is a statement in the current implementation:

```yar
match expr {
case Expr.Int(v) {
    print_int(v.value)
}
case Expr.Name(v) {
    print(v.text)
}
}
```

Current enum restrictions:

- case names must be unique within an enum
- payload field names must be unique within a case
- `match` requires explicit exhaustive arms
- enum values do not currently support `==` or `!=`
- direct recursive enum containment is rejected; use pointers for recursive shapes

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
- taking addresses of addressable elements with `&array[i]`
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
- taking addresses of addressable elements with `&slice[i]`
- `len(slice)`
- `append(slice, value)` returning an updated slice

Slices are views over runtime-managed backing storage. Slicing shares storage,
and `append` may reuse that storage or allocate a new backing buffer.

Slice indexing and slicing are bounds-checked at runtime and trap on invalid ranges.

## Maps

Maps are built-in associative containers:

```yar
counts := map[str]i32{"main": 1}
counts["check"] = 2

if has(counts, "main") {
    x := counts["main"]?
    print_int(x)
}

delete(counts, "check")
print_int(len(counts))
```

Supported map operations:

- map types: `map[K]V`
- map literals: `map[K]V{key1: value1, key2: value2}`
- element assignment: `m[key] = value`
- element lookup: `m[key]` returns `!V` (yields `error.MissingKey` if key is absent)
- `has(m, key)` returns `bool`
- `delete(m, key)` returns `void`
- `len(m)` returns `i32`

Supported key types: `bool`, `i32`, `i64`, `str`.

Map values are heap-allocated opaque handles. Map lookups return `!V` and compose
with `?` and `or |err| { ... }` like any other errorable expression.

There are no:

- iteration
- ordering guarantees
- set syntax

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

## Pointers

Pointers are explicit and typed:

```yar
struct Node {
    value i32
    next *Node
}

fn set_value(node *Node, value i32) void {
    (*node).value = value
}
```

Supported pointer operations:

- pointer types: `*T`
- address-of on addressable values: `&x`, `&items[0]`, `&(*node).next`
- address-of on composite literals: `&Node{value: 1, next: nil}`
- `nil`
- dereference: `*ptr`
- dereference assignment: `*ptr = value`
- pointer equality and inequality against `nil` or the same pointer type

Current pointer restrictions:

- there is no implicit dereference; use `(*ptr).field`, not `ptr.field`
- `nil` is only valid in pointer-typed contexts, so `p := nil` is rejected
- pointers do not support arithmetic, casts, or raw address exposure
- `*void` and `*noreturn` are rejected

## Statements

Implemented statements:

- block statements: `{ ... }`
- short local declarations: `x := expr`
- typed local declarations: `var name Type` and `var name Type = expr`
- assignment to locals, struct fields, array indices, slice indices, and dereferenced pointers
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
- `nil`
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
- unary `&`
- unary `*`
- short-circuit boolean operators: `&&`, `||`
- binary arithmetic: `+`, `-`, `*`, `/`, `%`
- binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`
- postfix error propagation: `expr?`
- local error handling: `expr or |err| { ... }`

### Integer Literals

Integer literals start as untyped integers and are coerced by context into `i32`
or `i64`.

### Strings

Supported string operations:

- `len(str)` returns the byte count as `i32`
- `s == t` and `s != t` compare strings by exact byte equality
- `s + t` concatenates two strings (allocates a new string)
- `s[i]` returns the byte value at offset `i` as `i32`
- `s[i:j]` returns the byte substring covering offsets `[i, j)` as `str`

Out-of-range string indexing and slicing trap at runtime.

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
- `len([N]T | []T | map[K]V | str) i32`
- `append([]T, T) []T`
- `has(map[K]V, K) bool`
- `delete(map[K]V, K) void`

They are not user-overridable.

## Runtime Behavior

When `main` returns `!i32`:

- success returns the `i32` value to the process
- error prints `unhandled error: <Name>` and exits with status `1`

If the runtime sees an unknown error code, it prints `unhandled error` and exits
with status `1`.

`panic(str)` writes to stderr and exits with status `1`.

Out-of-range slice indexing and invalid slice ranges trap with a runtime failure.

## Standard Library

The compiler ships with an embedded standard library written in yar. Stdlib
packages are imported like regular packages. If a local package with the same
name exists, it takes priority over the stdlib version.

### `strings`

```yar
import "strings"
```

Available functions:

- `strings.contains(s str, substr str) bool`
- `strings.has_prefix(s str, prefix str) bool`
- `strings.has_suffix(s str, suffix str) bool`
- `strings.index(s str, substr str) i32` — returns `-1` if not found
- `strings.count(s str, substr str) i32`
- `strings.repeat(s str, n i32) str`
- `strings.replace(s str, old str, new str, n i32) str` — `n < 0` replaces all
- `strings.trim_left(s str, cutset str) str`
- `strings.trim_right(s str, cutset str) str`
- `strings.join(parts []str, sep str) str`
- `strings.from_byte(i32) str` — construct a single-byte string from a byte value
- `strings.parse_i64(str) !i64` — parse a base-10 signed integer

### `utf8`

```yar
import "utf8"
```

Available functions:

- `utf8.decode(s str, off i32) !i32` — decode the rune at byte offset `off`
- `utf8.width(s str, off i32) !i32` — byte width of the rune at byte offset `off`
- `utf8.is_letter(r i32) bool` — classify a decoded rune as a letter or underscore
- `utf8.is_digit(r i32) bool` — classify a decoded rune as an ASCII digit
- `utf8.is_space(r i32) bool` — classify a decoded rune as whitespace

UTF-8 errors return `error.InvalidUTF8`. Out-of-range offsets return
`error.OutOfRange`.

### `conv`

```yar
import "conv"
```

Available functions:

- `conv.to_i64(n i32) i64` — widen an i32 to i64
- `conv.to_i32(n i64) i32` — truncate an i64 to i32
- `conv.byte_to_str(b i32) str` — construct a one-byte string from a byte value (traps if value is outside 0–255)
- `conv.itoa(n i32) str` — convert an i32 to its base-10 decimal string
- `conv.itoa64(n i64) str` — convert an i64 to its base-10 decimal string

### `path`

```yar
import "path"
```

Available functions:

- `path.clean(p str) str`
- `path.join(parts []str) str`
- `path.dir(p str) str`
- `path.base(p str) str`
- `path.ext(p str) str`

Current implementation notes:

- path helpers normalize `\` to `/`
- joined and cleaned paths use `/` separators

### `fs`

```yar
import "fs"
```

Types:

- `fs.DirEntry { name str, is_dir bool }`
- `fs.EntryKind { File, Directory, Other }`

Available functions:

- `fs.read_file(path str) !str`
- `fs.write_file(path str, data str) !void`
- `fs.read_dir(path str) ![]fs.DirEntry`
- `fs.stat(path str) !fs.EntryKind`
- `fs.mkdir_all(path str) !void`
- `fs.remove_all(path str) !void`
- `fs.temp_dir(prefix str) !str`

Filesystem errors surface through ordinary YAR errors using the names:

- `error.NotFound`
- `error.PermissionDenied`
- `error.AlreadyExists`
- `error.InvalidPath`
- `error.IO`

Current implementation note: the host filesystem runtime is POSIX-oriented and
uses `TMPDIR` or `/tmp` for `fs.temp_dir`.

### `process`

```yar
import "process"
```

Types:

- `process.Result { exit_code i32, stdout str, stderr str }`

Available functions:

- `process.args() []str`
- `process.run(argv []str) !process.Result`
- `process.run_inherit(argv []str) !i32`

Host-process launch failures surface through ordinary YAR errors using the names:

- `error.NotFound`
- `error.PermissionDenied`
- `error.InvalidArgument`
- `error.IO`

If a child process launches successfully, a non-zero child exit code is reported
as data in `process.Result.exit_code` or the returned `i32`, not as a YAR
`error`.

### `env`

```yar
import "env"
```

Available functions:

- `env.lookup(name str) !str`

Environment lookup returns `error.NotFound` when a variable is absent. Names
that cannot cross the host boundary return `error.InvalidArgument`.

### `stdio`

```yar
import "stdio"
```

Available functions:

- `stdio.eprint(msg str) void`

## Not Implemented

The compiler does not currently implement:

- methods
- import aliases
- generics
- closures or lambdas
- exceptions
- automatic recovery
