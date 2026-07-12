# YAR

This document tracks the language that the compiler implements today.

It is intentionally descriptive, not aspirational. If the compiler and this
document disagree, the compiler is the source of truth and this file should be
updated.

## Scope

- Multi-file packages
- Entry `package main` plus imported packages
- Top-level `struct`, `interface`, `enum`, `error`, and `fn` declarations, with optional `pub`
- Explicit generic structs and functions
- Function types and anonymous function literals
- Native code generation through LLVM IR plus `clang` (overridable via `CC` environment variable)
- Cross-compilation via `YAR_OS` and `YAR_ARCH` environment variables

Current implementation note: the shipped CLI is the Rust 2024 `yar` binary from
`crates/yar-cli`. It supports `check`, `emit-ir`, `build`, host `run`, host
`test`, `init`, and dependency manifest, lock, fetch, and update commands. The
Rust compiler path produces a checked program after semantic analysis; `check`
stops there, while code-producing commands explicitly invoke LLVM emission.
The shared `crates/yar-process-control` boundary applies typed tool-start
errors, absolute deadlines, and descendant containment to external build, test,
and Git processes.
It enforces package export visibility for imported declarations and exported
API types, and its LLVM emitter has clang-accepted coverage for
every current `testdata/**/main.yar` entry program, including host-backed `fs`,
`process`, `env`, `stdio`, and `net` runtime calls. Native paths link a Rust
runtime bundle. `YAR_RUNTIME_BUNDLE` selects a directory containing a strict
`yar-runtime.toml` manifest and one static archive. The CLI validates the target
triple, bundle format, runtime ABI, compiler compatibility epoch, archive path,
and ordered system-library list before invoking `clang`. Packaged compilers
discover `runtimes/<target-triple>/` beside the executable; source-tree host
builds use the same checked-in manifests with a Cargo-built archive. Cross
builds require a matching explicit or installed bundle. Native builds use the
Rust runtime only.

## File Shape

A valid entry package:

- starts with `package main`
- may contain one or more `.yar` files in the same directory
- may contain zero or more `import "path"` declarations after the package clause
- contains zero or more top-level `struct` declarations
- contains zero or more top-level `interface` declarations
- contains zero or more top-level `enum` declarations
- contains zero or more top-level `error` declarations
- contains zero or more top-level `fn` declarations
- must define `main`

Imported packages:

- use `import "path"` with slash-separated package paths
- must declare a package name matching the final import path segment
- may expose top-level `struct`, `interface`, `enum`, `error`, and `fn` declarations with `pub`
- use the reserved `std/<package>` namespace for embedded standard-library
  packages; these imports bypass project and dependency lookup
- otherwise resolve within the importing source origin: same-origin packages →
  aliases declared directly by that origin → error
- cannot share a final path segment with another distinct import in the same
  package, because that segment is the source qualifier

The compiler identifies a loaded package as `PackageId = (source origin,
source-relative subpath)`. The entry tree, each path dependency, each pinned git
source, and embedded stdlib are distinct origins. Import strings and dependency
aliases are bindings into that identity space; they are not package identity.
Package graphs, lowering, and cycle checks retain `PackageId`, and lowered
symbols use origin-safe canonical names.

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
- channel types such as `chan[i32]`
- function types such as `fn(i32) i32` and `fn(str) !i32`

### Error-Related Types

- `!T` means either a success value of type `T` or an error.
- `!void` is valid and represents an operation that either succeeds or returns an error.
- Plain `error` is also a valid type for non-`main` functions, parameters, locals, and fields.
- The compiler also produces first-class `!T` values for taskgroup results such as `taskgroup []!i32 { ... }`.

Current restrictions:

- parameters cannot use `void`
- parameters cannot use `noreturn`
- functions cannot use `!noreturn`
- functions cannot use `!error`
- struct fields cannot use `void`
- struct fields cannot use `noreturn`
- array elements cannot use `void`
- array elements cannot use `noreturn`
- slice elements cannot use `noreturn`
- `[]void` is valid and is primarily used as a taskgroup result type
- channel elements cannot use `void`
- channel elements cannot use `noreturn`
- channel elements cannot use first-class errorable types such as `!i32`
- channel elements cannot use another `chan[U]`
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
- the Rust runtime may reclaim unreachable managed storage during allocation
- collection is conservative and non-moving; programs cannot observe or depend
  on its exact timing

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

An initializer-free `var name T` is valid only when `T` has an implicit zero
value accessible in the current package. Implicit zeros are:

- `false` for `bool`, `0` for `i32` and `i64`, and `""` for `str`
- `nil` for pointers
- an empty nil-backed slice
- a nil interface
- a closed nil channel
- a recursively eligible fixed array or struct

Maps, function values, errors, and enums require explicit initializers. Use an
empty map literal such as `map[str]i32{}` when an initialized empty map is
needed. Errorable `!T` values have no implicit zero and must be handled before
ordinary local binding.

## Structs

User-defined structs are declared at top level:

```
struct User {
    id i32
    pub name str
}
```

Struct fields are package-private by default. Prefix a field with `pub` to make
it accessible outside the struct's declaring package. There is no `opaque`
keyword: an exported struct whose fields are all private is an opaque,
package-owned type by ordinary field visibility.

Supported struct operations:

- field access: `user.name`
- keyed literals: `User{id: 1, name: "bob"}`
- empty literals: `User{}`
- field assignment: `user.name = "eve"`

Within the declaring package, every field may be read, assigned, used as a
compound-assignment target, or addressed, and struct literals may initialize
private fields. Function literals defined there retain the same package
authority. Outside that package, those selector operations are available only
for `pub` fields. If a struct has any private field, all struct-literal
construction of that type is package-owned, including empty literals and
literals that mention only public fields. Exported constructors and methods are
the intended external boundary for such types.

Generic structs preserve each declared field's visibility in every concrete
instantiation. Package ownership follows the generic declaration, not the
instantiating package.

Implicit zero construction follows the same package boundary recursively. A
struct is implicitly zeroable only when every field type is zeroable in the
current package and every private field belongs to that package. Owning packages
may zero their own private and resource structs. Importers cannot zero structs
with private fields, including through containing arrays, wrappers, or omitted
aggregate slots.

Each omitted struct-literal field must have an accessible implicit zero. Fields
with explicit values do not impose that requirement. Generic structs apply the
rule to instantiated field types while ownership remains with the generic
declaration package.

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

Top-level and struct-field visibility use `pub`:

```
pub struct User {
    id i32
    pub name str
}

pub fn lookup() User {
    return User{id: 1, name: "Ada"}
}
```

Exported declarations cannot expose package-local types (struct, interface, or
enum) through public fields, parameters, or return types. Private fields may use
package-local types because they are not part of the exported surface.

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

Enums have no implicit zero case. An initializer-free enum local is invalid;
source code must choose a case explicitly. Omitted enum payload fields follow
the same implicit-zero rule as omitted struct fields.

Supported enum operations:

- plain cases such as `TokenKind.Ident`
- payload constructors such as `Expr.Name{text: "main"}` (keyed form)
- positional constructors for single-field cases such as `Expr.Name("main")`
- exhaustive `match`
- payload binding inside `match` arms with `case Expr.Name(v) { ... }`
- payload ignore binding with `case Expr.Int(_) { ... }`

Enum payload field syntax and visibility are unchanged. Payload fields are
inherently public. Writing `pub` inside an enum case payload produces
`enum payload fields are inherently public and do not accept 'pub'`.

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

An `else` arm can replace any number of unmatched cases:

```
match color {
case Color.Red { return 1 }
else { return 0 }
}
```

Current enum restrictions:

- case names must be unique within an enum
- payload field names must be unique within a case
- `match` requires exhaustive arms or an `else` wildcard
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

Array reads, assignments, and element address-taking are bounds-checked at
runtime and trap with `runtime failure: array index out of range`.

A zero-length array is implicitly zeroable for any element type. A
positive-length array is implicitly zeroable only when its element type is.
Array literals may use non-zeroable element types when every element is
provided; an omitted tail requires an accessible implicit zero for the element
type.

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
- slicing with `s[i:j]`, `s[i:]` (end defaults to `len(s)`), and `s[:j]` (start
  defaults to `0`)
- taking addresses of addressable elements with `&slice[i]`
- `len(slice)`
- `append(slice, value)` returning an updated slice

Slices are views over runtime-managed backing storage. Slicing shares storage,
and `append` may reuse that storage or allocate a new backing buffer. The
runtime may reclaim backing storage after it becomes unreachable.

Slice indexing and slicing are bounds-checked at runtime and trap on invalid ranges.

The implicit zero slice is empty and nil-backed regardless of element type.
An empty slice literal is an explicit empty value.

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

Map storage is runtime-managed. Unreachable maps and their internal storage may
be reclaimed by the runtime collector.

Maps have no implicit zero value. Use an explicit map literal, including
`map[K]V{}` for an initialized empty map.

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
- address-of on addressable values: `&x`, `&items[0]`, `&node.next`
- address-of on composite literals: `&Node{value: 1, next: nil}`
- `nil`
- dereference: `*ptr`
- dereference assignment: `*ptr = value`
- implicit dereference for field access: `ptr.field` (equivalent to `(*ptr).field`)
- implicit dereference for field assignment: `ptr.field = value`
- pointer equality and inequality against `nil` or the same pointer type
- dereferencing `nil`, including through implicit field access, terminates with
  `runtime failure: nil pointer dereference`

Current pointer restrictions:

- `nil` is only valid in pointer-typed contexts, so `p := nil` is rejected
- pointers do not support arithmetic, casts, or raw address exposure
- `*void` and `*noreturn` are rejected

## Statements

Implemented statements:

- block statements: `{ ... }`
- short local declarations: `x := expr`
- typed local declarations: `var name Type` and `var name Type = expr`
- assignment to locals, struct fields, array indices, slice indices,
  dereferenced pointers, and map elements
- compound assignment: `+=`, `-=`, `*=`, `/=`, `%=` for non-map assignment
  targets; the target is evaluated exactly once
- map compound assignment is rejected because lookup is errorable; handle the
  lookup explicitly before assigning a replacement value
- `if`
- `if` / `else`
- `else if`
- `for { ... }`
- `for cond { ... }`
- `for init; cond; post { ... }`
- `break`
- `continue`
- `return`
- `spawn call(...)` inside a taskgroup body
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
- `taskgroup []R { ... }`
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

## Concurrency

Structured concurrency is available through taskgroups and bounded channels.

### Taskgroups

`taskgroup []R { ... }` is an expression that spawns concurrent calls and
returns a result slice in spawn order. Each successful `spawn` starts one
native thread immediately; the taskgroup joins every task before it yields.

```yar
fn square(v i32) i32 {
    return v * v
}

fn main() i32 {
    values := taskgroup []i32 {
        spawn square(2)
        spawn square(3)
    }
    print(to_str(values[0]) + "\n")
    print(to_str(values[1]) + "\n")
    return 0
}
```

Taskgroup rules:

- the annotation must be a slice type
- each `spawn` target must be a named function call or an immediately called
  inline function literal; arbitrary function values, builtins, and methods
  cannot be spawned
- direct host-intrinsic spawns require a dedicated task wrapper; currently
  only `fs.read_file` has one, and other host calls must be wrapped in an
  inline function literal
- the spawned call must return the taskgroup element type exactly
- spawn arguments and inline-literal captures must be share-safe: scalars,
  `str`, `error`, fixed arrays, enums, non-resource structs, typed `net.Conn`
  and `net.Listener` references, `!T`, and `chan[T]` compose only from other
  share-safe types; `!void` is also share-safe
- pointers, slices, maps, interfaces, function values, file resource structs,
  and values that contain them cannot cross the spawn boundary
- the share-safety restriction does not apply to results because the parent
  observes them only after the taskgroup joins
- `spawn` is only valid inside the taskgroup body
- `spawn` is rejected inside a function literal nested under that taskgroup body
- `return` is not currently allowed inside a taskgroup body
- `noreturn` expression statements are rejected inside taskgroup bodies because
  they would bypass the mandatory join
- `?` is rejected inside a taskgroup body because propagation could bypass the
  taskgroup join; nested function literals may propagate from their own bodies
- `break` and `continue` may not exit through an enclosing loop outside the taskgroup body
- tasks start when `spawn` executes, but the taskgroup expression waits for all tasks before yielding its result
- `taskgroup []void` is valid for side-effecting tasks
- `taskgroup []!T` is valid and yields first-class errorable values

Arguments and captures cross the task boundary through shallow value copies,
not deep copies. The transitive share-safety rule prevents those copies from
carrying mutable aliases into another thread. Channels are the synchronized
exception. Bare `i64` values are scalars, so the checker cannot distinguish an
ordinary integer from a raw runtime or OS handle represented as `i64`.
Runtime-backed handles are kind-checked registry IDs with synchronized state,
so invalid or concurrently used IDs do not become unchecked pointer
dereferences. This runtime validation does not add nominal handle types or
compile-time provenance, and raw handles remain outside the intended
share-safe source model.

### Channels

`chan[T]` is a builtin bounded FIFO channel type.

```yar
jobs := chan_new[i32](4)
results := chan_new[i32](4)
```

Channel builtins:

- `chan_new[T](capacity i32) chan[T]`
- `chan_send(ch chan[T], value T) !void`
- `chan_recv(ch chan[T]) !T`
- `chan_close(ch chan[T]) void`

Channel rules:

- channel element types may not be `void`, `noreturn`, first-class errorable
  types, or another channel type
- taskgroup and channel element sizes use a checked signed 64-bit runtime ABI
- channels support `==` and `!=` identity comparison
- `chan_send` and `chan_recv` use `error.Closed` for closed-channel failures
- the implicit zero channel is closed: send and receive return `error.Closed`,
  and close is a no-op
- the native-thread runtime supports Linux, macOS, and Windows GNU
- unreachable channel tokens finalize their synchronized external state;
  buffered values remain collector roots only while the channel is live
- each taskgroup join consumes its internal handle, and task accounting rejects
  overflow or underflow
- concurrent runtime output is atomic per call; fatal paths omit diagnostics
  while any task is unjoined and immediately terminate the process without
  waiting for output or running shutdown handlers

### Integer Literals

Integer literals start as untyped integers and are coerced by context into `i32`
or `i64`. Binary expressions on untyped integer literals (e.g., `0 - 1`) remain
untyped until a concrete type is required, allowing `var x i64 = 0 - 1`.

### Integer Arithmetic

`i32` and `i64` use two's-complement arithmetic:

- `+`, `-`, `*`, and unary `-` wrap to the operand width
- compound `+=`, `-=`, and `*=` use the same wrapping behavior
- `/` and `%` trap when the divisor is zero
- `/` and `%` also trap for the signed overflow pair `MIN` and `-1`

The zero-divisor trap is `runtime failure: integer division or remainder by
zero`. The overflow trap is `runtime failure: integer division or remainder
overflow`.

### Strings

Supported string operations:

- `len(str)` returns the byte count as `i32`
- `s == t` and `s != t` compare strings by exact byte equality
- `s + t` concatenates two strings (allocates a new string)
- `s[i]` returns the byte value at offset `i` as `i32`
- `s[i:j]`, `s[i:]`, and `s[:j]` return the byte substring as `str` (omitted
  start defaults to `0`, omitted end defaults to `len(s)`)

Out-of-range string indexing and slicing trap at runtime.

Supported escapes:

- `\n` — newline
- `\t` — tab
- `\r` — carriage return
- `\0` — null byte
- `\\` — backslash
- `\"` — double quote

### Character Literals

Character literals use single quotes and produce an `i32` value representing
the Unicode code point:

```yar
x := 'a'      // 97
y := '\n'      // 10
z := '\''      // 39
```

Supported escapes in character literals: `\n`, `\t`, `\r`, `\0`, `\\`, `\'`.

### Boolean Operators

- `a && b` evaluates `b` only when `a` is `true`
- `a || b` evaluates `b` only when `a` is `false`
- both operands must be non-errorable `bool`
- the result type is `bool`

## Error Model

Errors are explicit values. There are no exceptions, hidden stack unwinding, or
`try`/`catch` semantics.

Named errors are package-level declarations. They are private by default:

```yar
error Internal
pub error Public
```

The declaring package refers to either declaration as `error.Name`. An importer
refers to a public error through its import qualifier, such as `storage.Public`.
Unknown names are compile-time errors; writing a name does not create an error.

Named error identity includes the origin-safe declaring package. Two packages
may declare the same leaf name without making equal values. Declarations in the
same package refer to one identity even when reached through different valid
import paths.

Named error expressions are valid as:

- the direct operand of `return` inside an errorable function or a function returning plain `error`
- a general expression producing a value of type `error`
- an operand in `==` or `!=` comparisons with other error values

Private errors may propagate through an exported errorable function, but
callers cannot name or construct them. Callers can still propagate them, bind
them in `or |err|`, compare obtained values, and stringify them.

The compiler assigns each canonical declaration identity a deterministic,
non-zero code within one compiled program. Codes are not stable across programs
and are not a host ABI. `to_str` preserves the legacy `"error.Name"` display,
so stringification is not an identity operation when two packages use the same
leaf name.

`error.MissingKey` and `error.Closed` are compiler-owned shared declarations
for maps, channels, and closed or invalid resources. They cannot be redeclared.

## Error Handling Forms

### Direct Return

An errorable call may be returned directly from a function with the same
errorable result type:

```
error Boom

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

Raw errorable call expressions cannot be used directly in:

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

First-class `!T` values produced by the language, such as `taskgroup []!T`
elements after indexing, may be handled later with `?` or `or |err| { ... }`.

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
- `sb_new() i64` — create a new string builder (returns opaque handle)
- `sb_write(i64, str) void` — append a string to the builder
- `sb_string(i64) str` — extract the built string and reset the builder
- `chan_new[T](i32) chan[T]` — create a bounded channel
- `chan_send(chan[T], T) !void` — send one value
- `chan_recv(chan[T]) !T` — receive one value
- `chan_close(chan[T]) void` — close a channel

They are not user-overridable.

String-builder handles are positive, process-local opaque `i64` tokens backed by
generation-tagged registry slots. Their mutable state is synchronized. Passing
an unknown, stale-generation, or wrong-kind token to a string-builder operation
terminates with `runtime failure: invalid string builder`.

Across runtime handle kinds, removing an entry advances its slot generation and
changes the full token before that slot can be reused. Stale-generation and
wrong-kind access does not consume the current entry. A slot whose maximum
generation has been removed is retired rather than wrapped.

## Runtime Behavior

When `main` returns `!i32`:

- success returns the `i32` value to the process
- error prints `unhandled error: <Name>` and exits with status `1`

If the runtime sees an unknown error code, it prints `unhandled error` and exits
with status `1`.

`panic(str)` writes to stderr and exits with status `1`.

Out-of-range slice indexing and invalid slice ranges trap with a runtime failure.

## Standard Library

The compiler ships with an embedded standard library written in Yar. Its
compiler-owned import namespace is `std/<package>`. The resolver selects that
namespace before project-local or dependency lookup, so those sources cannot
replace direct or transitive stdlib packages. Stdlib packages use the same
canonical paths for their internal imports. A missing bare import that names a
known stdlib package receives a diagnostic pointing to its `std/...` path;
genuine user-owned packages may still use those bare names.

### `strings`

```
import "std/strings"
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
- `strings.trim(s str, cutset str) str` — strip both ends
- `strings.split(s str, sep str) []str` — split string by separator
- `strings.to_lower(s str) str` — ASCII lowercase conversion
- `strings.to_upper(s str) str` — ASCII uppercase conversion
- `strings.join(parts []str, sep str) str`
- `strings.from_byte(i32) str` — construct a single-byte string from a byte value
- `strings.parse_i64(str) !i64` — parse a base-10 signed integer; returns
  `strings.InvalidInteger` or `strings.IntegerOverflow`

### `utf8`

```
import "std/utf8"
```

Available functions:

- `utf8.decode(s str, off i32) !i32` — decode the rune at byte offset `off`
- `utf8.width(s str, off i32) !i32` — byte width of the rune at byte offset `off`
- `utf8.is_letter(r i32) bool` — classify a decoded rune as a letter or underscore
- `utf8.is_digit(r i32) bool` — classify a decoded rune as an ASCII digit
- `utf8.is_space(r i32) bool` — classify a decoded rune as whitespace

UTF-8 errors return `utf8.InvalidUTF8`. Out-of-range offsets return
`utf8.OutOfRange`.

### `conv`

```
import "std/conv"
```

Available functions:

- `conv.to_i64(n i32) i64` — widen an i32 to i64
- `conv.to_i32(n i64) i32` — truncate an i64 to i32
- `conv.byte_to_str(b i32) str` — construct a one-byte string from a byte value (traps if value is outside 0–255)
- `conv.itoa(n i32) str` — convert an i32 to its base-10 decimal string
- `conv.itoa64(n i64) str` — convert an i64 to its base-10 decimal string

### `sort`

```
import "std/sort"
```

Available functions:

- `sort.strings(values []str) void` — sort a string slice in place by ascending bytewise lexicographic order
- `sort.i32s(values []i32) void` — sort an i32 slice in place in ascending order
- `sort.i64s(values []i64) void` — sort an i64 slice in place in ascending order

Current implementation note: these helpers use a simple in-place insertion sort.

### `path`

```
import "std/path"
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
import "std/fs"
```

Types:

- `fs.DirEntry { pub name str, pub is_dir bool }`
- `fs.EntryKind { File, Directory, Other }`
- `fs.File` — package-owned resource value with a private runtime handle

`fs.File` is a resource struct and cannot be passed to or captured by a
spawned task.

Its `handle` is a kind-checked, generation-tagged process-local opaque `i64`
token, not a native address. File operations synchronize access to the
underlying file. Closing removes the token so new lookups fail, then waits for
an operation holding the file lock before releasing the host file; it does not
interrupt blocking I/O. Close does not call an implicit durability sync.
Unknown, stale-generation, and wrong-kind tokens produce `error.Closed`.

Available functions:

- `fs.read_file(path str) !str`
- `fs.write_file(path str, data str) !void`
- `fs.read_dir(path str) ![]fs.DirEntry`
- `fs.stat(path str) !fs.EntryKind`
- `fs.mkdir_all(path str) !void`
- `fs.remove_all(path str) !void`
- `fs.temp_dir(prefix str) !str`
- `fs.open_read(path str) !fs.File`
- `fs.open_write(path str) !fs.File`

Methods on `fs.File`:

- `read(max_bytes i32) !str` — read up to `max_bytes`; returns empty string on
  EOF
- `write(data str) !i32` — write data and return bytes written
- `close() !void` — close the host file handle

Filesystem errors surface through package-owned public errors:

- `fs.NotFound`
- `fs.PermissionDenied`
- `fs.AlreadyExists`
- `fs.InvalidPath`
- `fs.InvalidArgument`
- `error.Closed`
- `fs.IO`

`fs.temp_dir` uses `TMPDIR` or `/tmp` on Unix and the system temporary
directory on Windows.

### `io`

```
import "std/io"
```

Interfaces:

- `io.Reader { read(max_bytes i32) !str }`
- `io.Writer { write(data str) !i32 }`
- `io.Closer { close() !void }`
- `io.ReadCloser { read(max_bytes i32) !str, close() !void }`
- `io.WriteCloser { write(data str) !i32, close() !void }`
- `io.ReadWriter { read(max_bytes i32) !str, write(data str) !i32 }`

Available functions:

- `io.copy(dst io.Writer, src io.Reader, chunk_size i32) !i64`
- `io.read_all(src io.Reader, chunk_size i32, max_bytes i32) !str`
- `io.close_quiet(c io.Closer) void`

`io.copy` and `io.read_all` use `io.InvalidArgument`; `io.copy` uses `io.IO`
for a short write, and `io.read_all` uses `io.LimitExceeded` when its bound
would be exceeded. Errors from the supplied reader, writer, or closer preserve
their original identities.

### `process`

```
import "std/process"
```

Types:

- `process.Result { pub exit_code i32, pub stdout str, pub stderr str }`
- `process.Limits` — package-owned validated execution limits
- `process.Cancellation` — package-owned share-safe cancellation signal

Available functions:

- `process.args() []str`
- `process.limits(timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64) !process.Limits`
- `process.cancellation() process.Cancellation`
- `process.cancel(value process.Cancellation) void`
- `process.run(argv []str, limits process.Limits, cancellation process.Cancellation) !process.Result`
- `process.run_inherit(argv []str, timeout_milliseconds i64, cancellation process.Cancellation) !i32`

Host-process launch failures surface through package-owned public errors:

- `process.NotFound`
- `process.PermissionDenied`
- `process.InvalidArgument`
- `process.Timeout`
- `process.LimitExceeded`
- `process.Cancelled`
- `process.IO`

If a child process launches successfully, a non-zero child exit code is reported
as data in `process.Result.exit_code` or the returned `i32`, not as a YAR
`error`.

Timeouts must be between 1 millisecond and 24 hours. Each capture cap must be
between 0 and 64 MiB; zero permits no bytes, and the exact cap is allowed.
`run` drains stdout and stderr concurrently. A timeout, cancellation, or the
first byte beyond either cap terminates and reaps the leader and ordinary
descendants before returning, and partial output is discarded. Cleanup failure
returns `process.IO`. `run_inherit` has the same timeout and cancellation
semantics but no capture limits because it inherits stdio.

On Unix, a descendant that deliberately starts a new session may escape the
process-group boundary. Process controls are not a sandbox and impose no CPU,
address-space, file, network, or process-count quotas. A blocking call consumes
only its calling native task thread; sibling tasks continue and can close the
share-safe cancellation signal.

### `env`

```
import "std/env"
```

Available functions:

- `env.lookup(name str) !str`

Environment lookup returns `env.NotFound` when a variable is absent. Names
that cannot cross the host boundary return `env.InvalidArgument`. Other host
failures use `env.PermissionDenied` or `env.IO`.

### `stdio`

```
import "std/stdio"
```

Available functions:

- `stdio.eprint(msg str) void`

### `net`

```
import "std/net"
```

Types:

- `net.Addr { pub host str, pub port i32 }`
- `net.Conn` — opaque connection reference
- `net.Listener` — opaque listener reference

`net.Conn` and `net.Listener` have private handle fields, so ordinary struct
visibility prevents external field access and literal construction. They are
typed, share-safe references backed by
kind-checked, generation-tagged process-local opaque `i64` tokens. They may be
passed to or captured by spawned tasks. Raw `i64` network intrinsics are
compiler/runtime implementation details rather than public stdlib API.

A connection permits one reader and one writer at the same time. Concurrent
reads serialize with reads, and concurrent writes serialize with writes.
Closing linearizes when the registry entry is removed: new operations fail,
blocked accept/read/write calls wake with `error.Closed`, and close waits until
in-flight operations and the host resource have finished releasing.

Available functions:

- `net.listen_stream(host str, port i32) !net.Listener` — bind and listen on a
  TCP address; empty host means the IPv4 wildcard address
- `net.connect_stream(host str, port i32) !net.Conn` — resolve and connect
- `net.resolve(host str, port i32) !net.Addr` — DNS resolution; returns first
  IPv4 or IPv6 result

Methods on `net.Listener`:

- `accept() !net.Conn`
- `addr() !net.Addr`
- `close() !void`

Methods on `net.Conn`:

- `read(max_bytes i32) !str`
- `write(data str) !i32` — perform one host write and return its exact byte
  count, which may be shorter than `len(data)`
- `close() !void`
- `local_addr() !net.Addr`
- `remote_addr() !net.Addr`
- `set_read_deadline(millis i32) !void`
- `set_write_deadline(millis i32) !void`

`read(max_bytes)` accepts 1 through 67,108,864 bytes inclusive and returns an
empty string only on EOF. Read and write deadlines are relative, per-operation
socket timeouts; zero disables the timeout. Changing a timeout is not promised
to interrupt a syscall that is already running. DNS resolution and connection
creation are synchronous host calls and cannot be interrupted before a
connection handle exists. A sibling task can end blocked accept, read, or write
by closing the typed listener or connection.

Networking errors surface through package-owned public errors:

- `net.ConnectionRefused`
- `net.Timeout`
- `net.AddrInUse`
- `net.ConnectionReset`
- `net.NotFound` (DNS failure)
- `net.PermissionDenied`
- `net.InvalidArgument`
- `net.IO`
- `error.Closed`

### HTTP serving

There is no `std/http` package. The earlier server experiment was
withdrawn because it did not provide bounded incremental framing, strict
request and response validation, connection deadlines, or a safe resource
lifecycle. A future HTTP design requires a new accepted proposal and
adversarial socket tests before it can return to the standard library.

### `testing`

```
import "std/testing"
```

Types:

- `testing.T` — package-owned test state with private fields

Construction used by the generated runner:

- `testing.new(name str) *testing.T`

Methods on `*testing.T`:

- `t.fail(msg str) void` — mark test as failed with a message
- `t.log(msg str) void` — record a message (shown on failure)
- `t.has_failed() bool` — check if the test has failed
- `t.message_count() i32` — return the number of recorded messages
- `t.message(index i32) str` — return one recorded message

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
same package as the code under test. During normal compilation (`check`,
`emit-ir`, `build`, `run`), test files are excluded. `yar test` includes test
files only from the selected entry package. Imported packages and dependencies
are always loaded without their test files.

### Test Functions

Every function in an entry-package test file whose name starts with `test_`
must take one `*testing.T` parameter, return non-errorable `void`, and have no
receiver or type parameters. Invalid declarations produce source diagnostics
instead of being skipped.

```
import "std/testing"

fn test_addition(t *testing.T) void {
    testing.equal[i32](t, add(2, 3), 5)
}

fn test_greeting(t *testing.T) void {
    testing.equal[str](t, greet("world"), "hello world")
}
```

### Error Testing

Error values support `==` and `!=`, so tests can assert on specific errors:

```
error DivideByZero

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

## Compiler CLI

`yar --help` and `yar --version` report root information, and
`yar <command> --help` reports command-specific usage without resolving a
project, target, or external tool.

The host execution form is:

```bash
yar run <file-or-directory> [-- <argument>...]
```

When arguments are present, the `--` delimiter is required. Every value after
it is forwarded unchanged after the temporary executable's argv element zero.
The program inherits the invocation directory, environment, and standard
streams, and its numeric exit status becomes the `yar run` exit status.

External native-build, test-binary, and Git subprocesses use positive-integer
second limits from `YAR_BUILD_TIMEOUT_SECS`, `YAR_TEST_TIMEOUT_SECS`, and
`YAR_GIT_TIMEOUT_SECS`; their defaults are 30, 30, and 300 seconds. One build
deadline is shared by its Cargo and clang subprocesses, and one Git deadline is
shared by all Git subprocesses in a dependency command. The program launched by
`yar run` has no default deadline. Timed subprocesses use Unix process-group or
Windows Job Object containment so timeouts terminate ordinary descendants; a
Unix descendant that deliberately creates a new session is outside that
containment boundary.

These settings belong to the Rust CLI. Source-level `std/process` timeouts,
capture caps, and cancellation are explicit function arguments and are not
configured or overridden by the CLI environment variables.

## Dependencies

Yar supports external dependencies through `yar.toml` and `yar.lock` files.
Dependencies are git repositories, with no central registry.

### Project Selection

Use the optional global prefix before a command to select a project explicitly:

```bash
yar --manifest-path <path/to/yar.toml> <command> ...
```

The prefix may appear only once, must name `yar.toml`, and never falls back to
automatic discovery. Relative manifest paths are resolved from the invocation
directory. `init` and `add` may create an absent explicitly selected manifest
in an existing target directory; every other explicitly selected command
requires it to exist. A created manifest uses the selected directory's name
when it is a valid package alias and otherwise uses `myproject`.

Without the prefix, `check`, `emit-ir`, `build`, `run`, and `test` search from
the named entry file's parent or entry directory toward the filesystem root and
select the nearest `yar.toml`. If none exists, the entry directory is a
manifestless project root. An explicitly selected project root must contain the
entry package.

`add`, `remove`, `fetch`, `lock`, and `update` instead search from the
invocation directory. If no ancestor manifest exists, `add` creates one in that
directory while the other dependency commands fail. `init` deliberately does
not search ancestors; it creates a project in the invocation directory unless
an explicit manifest path selects another target.

A manifest candidate must be a valid regular file. An invalid nearest candidate
is an error rather than a reason to continue to a parent project. The selected
manifest's directory is the project root and owns its sibling `yar.lock`,
dependency-metadata recovery state, and root package tree. Manifest-declared
relative dependency paths use that directory. Relative manifest, source, and
output arguments still use the invocation directory, and project selection does
not change the working directory inherited by `yar run` or `yar test` programs.

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

Each dependency alias becomes the top-level import path segment. The alias
`std` is reserved for the compiler-owned standard library and is rejected:

```yar
import "http"
import "http/router"
```

Version specifiers (exactly one required for git dependencies):

- `tag` — a git tag (recommended)
- `rev` — an exact commit SHA
- `branch` — a branch name (development only)

### Lock File (`yar.lock`)

Auto-generated by `yar lock`. The file has explicit format `version = 1` and
records the complete reachable git dependency graph. Each package node pins an
exact commit and SHA-256 content hash. Its child edges repeat each child's
alias, git URL, and exact `tag`, `rev`, or `branch`; the target node records the
child's resolved commit and hash.

```toml
version = 1

[[package]]
name = "http"
git = "https://github.com/user/yar-http.git"
tag = "v0.3.1"
commit = "0123456789abcdef0123456789abcdef01234567"
hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[package.dependencies]]
name = "tls"
git = "https://github.com/user/yar-tls.git"
tag = "v0.2.0"

[[package]]
name = "tls"
git = "https://github.com/user/yar-tls.git"
tag = "v0.2.0"
commit = "89abcdef0123456789abcdef0123456789abcdef"
hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
```

Commit `yar.lock` to version control. Full commit values are 40 lowercase
hexadecimal characters and identify cache entries. Package nodes and child
edges are emitted in alias order. A missing or unsupported lock version is
rejected; run `yar lock` to regenerate it. Regeneration performs normal full
resolution, so review the lock diff when a tag or branch may have moved.
`yar fetch` does not perform that resolution: it verifies valid cached entries
offline and requests the locked commit object directly from the recorded Git
URL only when an entry is missing.

Before compilation opens a dependency cache, and before `yar fetch` accesses
the cache or network, Yar reconciles the manifest-derived roots and the lock
graph. Alias, git URL, ref kind, and ref value must match exactly. Duplicate
package aliases or child edges, missing targets, dependency cycles, and
unreachable package nodes are errors. There is no root override for source/ref
conflicts.

When an import selects a locked git dependency, the compiler verifies its cache
tree against the lock hash before reading its manifest or source, then checks
that the manifest's git dependencies exactly match the node's recorded child
edges. A missing, unreadable, symlinked, hash-mismatched, or edge-mismatched
selected tree is a hard package-loading error with repair guidance;
compilation does not repair the cache or substitute a same-named stdlib
package. Unused dependencies and dependencies shadowed by local packages do
not require a cache.

Local `path` dependencies remain live, unhashed filesystem inputs and may be
declared only in the root `yar.toml`. Relative values are resolved from the
directory containing that manifest, not from the invocation directory. A root
path dependency's manifest may contribute git roots to reconciliation, but may
not declare another path dependency. A locked git package may not declare a
path dependency. Selecting a declared path alias requires its directory to
exist; it does not fall back to a same-named standard-library package.

Alias visibility is scoped to the importing source origin. The entry origin
uses aliases from the root manifest, a root path dependency uses aliases from
its own manifest, and a locked git origin uses its recorded child edges. Lock
reachability alone does not make an alias importable. Source that previously
imported a reachable transitive alias must declare that alias directly in its
own manifest.

Lock v1 and the cache format are unchanged. Lock v1 still requires one global
source/ref tuple per alias, even across different owner scopes; allowing two
owners to reuse the same alias for different targets requires a future lock v2.

### Commands

Every command accepts the optional prefix
`yar --manifest-path <path/to/yar.toml> <command> ...`; it must appear before
the command rather than among command arguments.

| Command                            | Description                      |
| ---------------------------------- | -------------------------------- |
| `yar init`                         | Create `yar.toml`                |
| `yar add <alias> <url> --tag=v1.0` | Add dependency                   |
| `yar remove <alias>`               | Remove dependency                |
| `yar fetch`                        | Download dependencies to cache   |
| `yar lock`                         | Regenerate `yar.lock`            |
| `yar update [alias]`               | Re-resolve and update `yar.lock` |

`yar update <git-alias>` replaces the selected dependency's graph, preserves
unrelated nodes required by other roots, merges compatible shared aliases using
the updated resolution, and prunes nodes that are no longer reachable. It
refuses to write when an unselected root is stale or a shared alias would
resolve to a different source/ref tuple. A path dependency has no independent
locked revision, so `yar update <path-alias>` is rejected with guidance to run
`yar lock`.

### Metadata Publication

`yar add` and `yar remove` resolve, reconcile, and serialize the complete
target manifest and lock state before changing project metadata. They publish
`yar.toml` together with the target `yar.lock`, including deletion of the lock
when no git roots remain. `yar lock` and `yar update` publish only the lock
state and preserve the manifest byte-for-byte.

A journal beside the selected manifest records the prior contents or absence of
both files. A failure before commit restores that prior pair. Explicit selection
recovers only its fixed manifest directory and never falls back. Automatic
discovery recognizes a prepared journal or completion marker even when the live
manifest is absent, recovers that candidate, and restarts its ancestor search
before reading metadata. A prepared interrupted transaction is rolled back; a
transaction that reached its commit phase keeps the target pair while cleanup
finishes. Success messages are printed only after commit and cleanup.
Resolution may warm verified content-addressed global dependency-cache entries
before publication; those cache entries are outside the project-metadata
transaction and are not rolled back. Existing metadata-file permissions are
preserved. Do not run another Yar CLI command against the same selected project
while dependency metadata publication or recovery is in progress, even from
another invocation directory.

### Resolution Order

For each import:

1. A `std/<package>` path resolves only within the embedded stdlib origin.
2. Otherwise, check the importer's same-origin package tree, including its self
   alias.
3. Check an alias declared directly by that origin.
4. Report an error. If the unresolved bare path is a known stdlib package, the
   diagnostic names the required `std/<package>` spelling.

Imports originating in embedded stdlib also use `std/<package>` and cannot
consult project or dependency sources.

### Cache

Dependencies are cached globally under `$HOME/Library/Caches/yar/deps/` on
macOS and `$HOME/.cache/yar/deps/` on other supported hosts. Override the root
with `YAR_CACHE`.

`yar fetch` verifies existing entries before reporting success. For a missing
entry it fetches the locked commit directly, checks the checkout's HEAD, lock
hash, and manifest edges in temporary storage, and only then publishes the
cache entry. If the remote cannot provide that object, fetch fails without
falling back to the recorded tag, branch, or revision; `yar lock` or
`yar update` is required to select a different commit. Cached git trees may
contain only real directories and regular files; symlinks and special
filesystem entries are rejected. A corrupt cache fails closed and is not
deleted automatically. A graph with no effective git roots needs no
`yar.lock`; `yar fetch` succeeds without creating a dependency cache.

## Not Implemented

The compiler does not currently implement:

- import aliases
- exceptions
- general application/runtime recovery beyond dependency-metadata recovery
