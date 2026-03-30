# YAR

This document tracks the language that the compiler implements today.

It is intentionally descriptive, not aspirational. If the compiler and this
document disagree, the compiler is the source of truth and this file should be
updated.

## Scope

- Multi-file packages
- Entry `package main` plus imported packages
- Top-level `struct`, `interface`, `enum`, and `fn` declarations, with optional `pub`
- Explicit generic structs and functions
- Function types and anonymous function literals
- Native code generation through LLVM IR plus `clang` (overridable via `CC` environment variable)
- Cross-compilation via `YAR_OS` and `YAR_ARCH` environment variables

## File Shape

A valid entry package:

- starts with `package main`
- may contain one or more `.yar` files in the same directory
- may contain zero or more `import "path"` declarations after the package clause
- contains zero or more top-level `struct` declarations
- contains zero or more top-level `interface` declarations
- contains zero or more top-level `enum` declarations
- contains zero or more top-level `fn` declarations
- must define `main`

Imported packages:

- live in subdirectories under the entry package root, or are declared as
  dependencies in `yar.toml`, or are embedded stdlib packages
- use `import "path"` with slash-separated package paths
- must declare a package name matching the final import path segment
- may expose top-level `struct`, `interface`, `enum`, and `fn` declarations with `pub`
- resolution order: local → dependency → stdlib

`main` must return either:

- `i32`
- `!i32`

## Comments

The lexer supports line comments:

```
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
- user-defined `interface` types
- instantiated generic struct types such as `Box[i32]`
- user-defined `enum` types
- fixed-size array types such as `[4]i32` and `[3]User`
- slice types such as `[]i32` and `[]User`
- map types such as `map[str]i32` and `map[i32]bool`
- function types such as `fn(i32) i32` and `fn(str) !i32`

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

## Memory Management

Heap-backed values use runtime-managed storage.

- user code does not manually free memory
- there is no `gc()` builtin or `free(...)` operation
- allocation failure is an unrecoverable runtime failure, not a YAR `error`
- the runtime may reclaim unreachable heap-backed storage during allocation
- programs must not depend on exactly when reclamation happens

## Generics

The current implementation supports a narrow explicit generic system:

```
struct Box[T] {
    value T
}

fn first[T](values []T) T {
    return values[0]
}

fn main() i32 {
    box := Box[i32]{value: first[i32]([]i32{7, 9})}
    return box.value
}
```

Current generic rules:

- generic declarations are supported on top-level `struct` and `fn`
- every use site must supply explicit type arguments
- type arguments are ordinary Yar types such as `i32`, `str`, `[]User`, or `Box[i64]`
- generic structs may be used in fields, parameters, returns, locals, and literals
- generic functions may be called across packages with explicit type arguments
- instantiations are monomorphized before semantic checking and code generation
- generic code is type-checked after substitution at each instantiated use site

Current generic restrictions:

- there is no type-argument inference
- there are no constraints
- enums cannot declare type parameters
- methods cannot declare type parameters
- methods on instantiated generic types are not supported
- generic declarations cannot be referenced without instantiation
- generic declarations that are never instantiated are not fully type-checked

## Declarations

Local declarations:

```
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

```
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

```
struct Node {
    value i32
    next *Node
}
```

There are no:

- embedding
- tags

Top-level visibility uses `pub`:

```
pub struct User {
    id i32
}

pub fn lookup() User {
    return User{id: 1}
}
```

Exported declarations cannot expose package-local types (struct, interface, or
enum) through public fields, parameters, or return types.

## Interfaces

Interfaces describe required behavior through method signatures:

```
interface Writer {
    write(msg str) !void
}
```

Concrete values satisfy interfaces implicitly when the exact receiver type
provides matching methods:

```
struct Buffer {
    prefix str
}

fn (b Buffer) write(msg str) !void {
    print(b.prefix + msg)
    return
}

fn emit(w Writer) !void {
    return w.write("ok")
}
```

Current interface rules:

- interface bodies contain method requirements only; fields are not allowed
- interface methods may return ordinary or errorable results
- concrete satisfaction is implicit
- satisfaction uses the exact receiver type, so `T` and `*T` remain distinct
- interface values support method calls only through the declared method set
- interface values are not comparable
- interface-to-interface conversion is supported only for the same exact interface type
- `nil` remains pointer-only; it does not coerce to interface types
- calling a zero-valued interface panics with `nil interface method call`
- interfaces are not generic in the current implementation

## Enums

Enums model closed sets of named variants:

```
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

```
match expr {
case Expr.Int(v) {
    print(to_str(v.value))
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

```
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

```
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
and `append` may reuse that storage or allocate a new backing buffer. Unused
backing storage may be reclaimed later by the runtime.

Slice indexing and slicing are bounds-checked at runtime and trap on invalid ranges.

## Maps

Maps are built-in associative containers:

```
counts := map[str]i32{"main": 1}
counts["check"] = 2

if has(counts, "main") {
    x := counts["main"]?
    print(to_str(x))
}

delete(counts, "check")
print(to_str(len(counts)))
```

Supported map operations:

- map types: `map[K]V`
- map literals: `map[K]V{key1: value1, key2: value2}`
- element assignment: `m[key] = value`
- element lookup: `m[key]` returns `!V` (yields `error.MissingKey` if key is absent)
- `has(m, key)` returns `bool`
- `delete(m, key)` returns `void`
- `keys(m)` returns `[]K`
- `len(m)` returns `i32`

Map storage is runtime-managed. The runtime may reclaim unreachable maps and
their internal storage without any user-visible deallocation step.

Supported key types: `bool`, `i32`, `i64`, `str`.

Map values are heap-allocated opaque handles. Map lookups return `!V` and compose
with `?` and `or |err| { ... }` like any other errorable expression. `keys(m)`
returns a snapshot slice containing each present key exactly once, with no
ordering guarantee.

There are no:

- ordering guarantees
- live iterators
- set syntax

## Functions

Functions are declared with `fn`:

```
fn add(a i32, b i32) i32 {
    return a + b
}
```

Parameters are positional and explicitly typed.

Function values use explicit function types and anonymous literals:

```
fn make_adder(base i32) fn(i32) i32 {
    return fn(delta i32) i32 {
        return base + delta
    }
}
```

Current closure rules:

- function types are written as `fn(T1, T2) R` or `fn(T) !R`
- anonymous function literals use `fn(...) R { ... }`
- function values may be stored in locals, passed as parameters, returned, and called
- closures capture outer locals lexically by value at closure creation time
- captured outer locals are readable inside a closure but cannot be assigned there
- captured outer locals are not addressable, so closures cannot mutate captured state indirectly through pointers
- methods are still not first-class values, so `value.method` without `(...)` is rejected

Methods attach behavior to named struct types:

```
struct User {
    name str
}

fn (u User) label() str {
    return u.name
}

fn (u *User) rename(name str) void {
    (*u).name = name
}
```

Current method rules:

- methods are allowed only on named struct types declared in the same package
- receivers may be either `T` or `*T`
- method calls use `value.method(...)`
- pointer and value receivers do not auto-convert; match the receiver type explicitly
- interface satisfaction follows the same exact receiver matching
- methods are not first-class values, so `value.method` without `(...)` is rejected
- exported methods use `pub fn (...)`

There are no:

- variadics

Cross-package function calls stay qualified:

```
package main

import "lexer"

fn main() i32 {
    return lexer.exit_code()
}
```

## Pointers

Pointers are explicit and typed:

```
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
- method calls
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

```
error.DivideByZero
```

`error.Name` is valid as:

- the direct operand of `return` inside an errorable function or a function returning plain `error`
- a general expression producing a value of type `error`
- an operand in `==` or `!=` comparisons with other error values

The checker records every distinct returned error name and assigns it a numeric
code for code generation.

## Error Handling Forms

### Direct Return

An errorable call may be returned directly from a function with the same
errorable result type:

```
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

```
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

```
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
- `panic(str) noreturn`
- `len([N]T | []T | map[K]V | str) i32`
- `append([]T, T) []T`
- `has(map[K]V, K) bool`
- `delete(map[K]V, K) void`
- `keys(map[K]V) []K`
- `to_str(i32 | i64 | bool | str | error) str`

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

The compiler ships with an embedded standard library written in Yar. Stdlib
packages are imported like regular packages. If a local package with the same
name exists, it takes priority over the stdlib version.

### `strings`

```
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

```
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

```
import "conv"
```

Available functions:

- `conv.to_i64(n i32) i64` — widen an i32 to i64
- `conv.to_i32(n i64) i32` — truncate an i64 to i32
- `conv.byte_to_str(b i32) str` — construct a one-byte string from a byte value (traps if value is outside 0–255)
- `conv.itoa(n i32) str` — convert an i32 to its base-10 decimal string
- `conv.itoa64(n i64) str` — convert an i64 to its base-10 decimal string

### `sort`

```
import "sort"
```

Available functions:

- `sort.strings(values []str) void` — sort a string slice in place by ascending bytewise lexicographic order
- `sort.i32s(values []i32) void` — sort an i32 slice in place in ascending order
- `sort.i64s(values []i64) void` — sort an i64 slice in place in ascending order

Current implementation note: these helpers use a simple in-place insertion sort.

### `path`

```
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

```
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

Current implementation note: the host filesystem runtime uses POSIX APIs on
Unix-like systems and Win32 APIs on Windows. `fs.temp_dir` uses `TMPDIR` or
`/tmp` on Unix and `GetTempPath` on Windows.

### `process`

```
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

```
import "env"
```

Available functions:

- `env.lookup(name str) !str`

Environment lookup returns `error.NotFound` when a variable is absent. Names
that cannot cross the host boundary return `error.InvalidArgument`.

### `stdio`

```
import "stdio"
```

Available functions:

- `stdio.eprint(msg str) void`

### `testing`

```
import "testing"
```

Types:

- `testing.T { name str, failed bool, messages []str }`

Methods on `*testing.T`:

- `t.fail(msg str) void` — mark test as failed with a message
- `t.log(msg str) void` — record a message (shown on failure)
- `t.has_failed() bool` — check if the test has failed

Assertions:

- `testing.equal[V](t *testing.T, got V, want V) void` — fail if `got != want`, with "got X, want Y" message via `to_str`
- `testing.not_equal[V](t *testing.T, got V, want V) void` — fail if `got == want`

`V` can be any `==`-comparable type: `i32`, `i64`, `bool`, `str`, or `error`.

Boolean assertions:

- `testing.is_true(t *testing.T, value bool) void`
- `testing.is_false(t *testing.T, value bool) void`

Explicit failure:

- `testing.fail(t *testing.T, msg str) void`

## Testing

The `yar test` command discovers, compiles, and runs test functions.

### Test Files

Test files use the `_test.yar` suffix (e.g., `math_test.yar`). They belong to the
same package as the code under test. During normal compilation (`build`, `run`,
`check`), test files are excluded.

### Test Functions

Test functions start with `test_`, take a single `*testing.T` parameter, and
return `void`:

```
import "testing"

fn test_addition(t *testing.T) void {
    testing.equal_i32(t, add(2, 3), 5)
}

fn test_greeting(t *testing.T) void {
    testing.equal_str(t, greet("world"), "hello world")
}
```

### Error Testing

Error values support `==` and `!=`, so tests can assert on specific errors:

```
fn test_divide_by_zero(t *testing.T) void {
    result := divide(10, 0) or |err| {
        testing.equal[error](t, err, error.DivideByZero)
        return
    }
    testing.fail(t, "expected error")
}
```

### Running Tests

```
yar test <path>
```

Output:

```
PASS: test_addition
FAIL: test_wrong_result
    got 4, want 5
PASS: test_greeting

2 passed, 1 failed
```

Exit code is `0` when all tests pass, `1` when any test fails.

## Dependencies

Yar supports external dependencies through `yar.toml` and `yar.lock` files.
Dependencies are git repositories, with no central registry.

### Manifest (`yar.toml`)

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
http = { git = "https://github.com/user/yar-http.git", tag = "v0.3.1" }
json = { git = "https://github.com/user/yar-json.git", rev = "a1b2c3d" }
local_lib = { path = "../my-local-lib" }
```

Each dependency alias becomes the top-level import path segment:

```yar
import "http"
import "http/router"
```

Version specifiers (exactly one required for git dependencies):

- `tag` — a git tag (recommended)
- `rev` — an exact commit SHA
- `branch` — a branch name (development only)

### Lock File (`yar.lock`)

Auto-generated by `yar lock`. Pins each git dependency to an exact commit SHA
and SHA-256 content hash. Commit to version control for reproducible builds.

### Commands

| Command                            | Description                      |
| ---------------------------------- | -------------------------------- |
| `yar init`                         | Create `yar.toml`                |
| `yar add <alias> <url> --tag=v1.0` | Add dependency                   |
| `yar remove <alias>`               | Remove dependency                |
| `yar fetch`                        | Download dependencies to cache   |
| `yar lock`                         | Regenerate `yar.lock`            |
| `yar update [alias]`               | Re-resolve and update `yar.lock` |

### Resolution Order

1. Local filesystem
2. Dependency index (from `yar.toml`)
3. Embedded stdlib
4. Error

### Cache

Dependencies are cached globally at `os.UserCacheDir()/yar/deps/`. Override
with `YAR_CACHE` environment variable.

## Not Implemented

The compiler does not currently implement:

- import aliases
- exceptions
- automatic recovery
