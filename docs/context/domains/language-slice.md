# Language Slice

## Source Shape

- A source file starts with a package declaration.
- Entry builds require a `package main` root package with `main` returning
  `i32` or `!i32`.
- A package is one or more files in one directory that declare the same package
  name.
- Imports are explicit `import "path"` declarations immediately after the
  package clause.
- Import paths are slash-separated logical package names. Absolute paths,
  dot-prefixed paths, empty segments, and invalid identifier segments are
  rejected.
- Top-level declarations may be `struct`, `enum`, `fn`, or receiver-style
  method declarations, optionally prefixed with `pub`.
- Top-level `struct` and `fn` declarations may declare explicit type
  parameters.
- Functions and methods have positional parameters and an explicit return type.
- Return types may be prefixed with `!` to mark the function as errorable.
- `let` is not supported; local declarations use `:=` or `var`.

## Implemented Types

- `bool`
- `i32`
- `i64`
- `str`
- `void`
- `noreturn`
- `error`
- typed pointer types
- user-defined struct types
- user-defined enum types
- fixed array types
- slice types
- map types

## Statements

- Block statements delimited by `{` and `}`
- `:=` bindings with inferred type from the assigned expression
- `var name Type`
- `var name Type = expr`
- Reassignment to an existing local, struct field, array index, slice index,
  dereferenced pointer, or map element
- `if`, `else`, and `else if`
- `for cond { ... }`
- `for init; cond; post { ... }`
- `break`
- `continue`
- `return`
- `match value { case Enum.Case { ... } ... }`
- Expression statements

## Expressions

- Local identifier lookup
- Package-qualified function calls such as `lexer.classify()`
- Method calls such as `user.display_name()`
- Integer literals with coercion into `i32` or `i64`
- String literals with `\n`, `\t`, `\\`, and `\"` escapes
- Boolean literals
- `nil`
- `error.Name` literals in return position
- Struct literals
- Enum case constructors such as `TokenKind.Ident` and
  `Expr.Name{text: "main"}`
- Array literals
- Slice literals
- Map literals
- Function calls
- Explicit generic function calls such as `first[i32](values)`
- Grouping with parentheses
- Field access
- Indexing
- Slicing with `s[i:j]`
- Address-of with `&expr` on addressable values and composite literals
- Dereference with `*expr`
- Postfix error propagation with `expr?`
- Local error handling with `expr or |err| { ... }`
- Unary operators: `-`, `!`, `&`, `*`
- Short-circuit boolean operators: `&&`, `||`
- Binary arithmetic: `+`, `-`, `*`, `/`, `%`
- Binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`

## Semantic Rules

- Entry `main` must exist and return `i32` or `!i32`.
- Imported package references must stay qualified.
- Generic uses must supply explicit type arguments; the language does not infer
  them.
- The current generic system has no constraints.
- Generic structs and generic functions are supported, but enums are not
  generic.
- Methods cannot declare type parameters, and methods on instantiated generic
  types are not supported.
- Methods are allowed only on named local struct types or pointers to named
  local struct types.
- Value receiver methods and pointer receiver methods are distinct; method
  calls require an exact receiver type match.
- Methods are not first-class values; `value.method` is rejected outside an
  immediate call.
- Local packages shadow stdlib packages with the same import path.
- Imported packages expose only `pub` top-level declarations.
- Exported functions, structs, enums, and methods cannot expose non-exported
  local struct or enum types in parameters, returns, fields, or receiver
  types.
- Duplicate top-level names are rejected package-wide, including across files.
- Import cycles are rejected.
- Enum case names must be unique within their enum.
- Payload field names must be unique within an enum case.
- Parameters cannot use `void`, `noreturn`, or an unknown type.
- Struct fields, array elements, and slice elements cannot use `void`,
  `noreturn`, or an unknown type.
- Enum payload fields cannot use `void`, `noreturn`, or an unknown type.
- Pointer targets cannot use `void`, `noreturn`, or an unknown type.
- Direct recursive struct or enum containment is rejected, but recursive shapes
  through `*T` and `[]T` remain valid.
- `noreturn` functions cannot also be errorable, cannot use `return`, and must
  not fall through.
- Plain `error` is a valid parameter or return type for non-`main` functions.
- Non-`void` functions must return on every reachable path.
- `if` and `for` conditions must be non-errorable `bool` expressions.
- Arithmetic and relational operators require matching integer operands after
  literal coercion.
- String `==` and `!=` compare by exact byte equality.
- String `+` concatenates two strings into a new heap-allocated string.
- String indexing `s[i]` returns the byte value at offset `i` as `i32`, with
  runtime bounds checking.
- String slicing `s[i:j]` returns the byte substring as `str`, with runtime
  bounds checking.
- Equality and inequality are supported for integers, `bool`, `str`,
  same-typed pointers, and pointer-vs-`nil`.
- Equality and inequality are not supported for enum values.
- `&&` and `||` require `bool` operands and evaluate the right operand only
  when needed.
- Unary `-` requires an integer operand.
- Unary `!` requires a `bool` operand.
- Address-of requires an addressable operand or a composite literal.
- Dereference requires a non-errorable pointer operand.
- Field access requires a struct value and a known field.
- Plain enum cases are values of their enum type.
- Payload enum cases are constructed with keyed field syntax and produce the
  enclosing enum type.
- Indexing an array or slice requires an integer index and returns the element
  type.
- Indexing a map requires a key of the map's key type and returns `!V`,
  yielding `error.MissingKey` on absent keys.
- Slicing requires a slice or `str` value and integer bounds.
- `match` requires a non-errorable enum value, each arm must use a case from
  that same enum, and every case must be covered exactly once.
- Payload bindings in `match` arms have a generated payload-struct type, and
  `_` ignores a payload.
- `nil` is valid only in pointer-typed contexts; `p := nil` is rejected because
  there is no pointer type to infer.
- Out-of-range slice indexing and slicing trap at runtime.
- Map key types are restricted to `bool`, `i32`, `i64`, and `str`.
- Map value types cannot be `void`, `noreturn`, or an unknown type.
- `m[key] = value` inserts or replaces the entry for `key` in a map.
- `len` requires an array, slice, map, or `str` argument and returns `i32`.
- `append` requires `append([]T, T)` and returns `[]T`.
- `error.Name` is only valid as the direct operand of `return` inside an
  errorable function or a function returning `error`.
- A raw errorable call cannot be used directly as a value; it must be returned
  directly, propagated with `?`, or handled with `or |err| { ... }`.
- `?` is only valid on `!T` or `error` expressions and only inside a function
  that can return an error.
- `or |err| { ... }` is only valid on `!T` or `error` expressions.
- The handler name in `or |err| { ... }` is scoped only to the handler block.
- When `or` is used on a value-producing `!T` expression, the handler block
  must terminate control flow.

## Builtins

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
- `len([N]T | []T | map[K]V | str) i32`
- `append([]T, T) []T`
- `has(map[K]V, K) bool`
- `delete(map[K]V, K) void`
- `keys(map[K]V) []K`

Builtins remain globally available and are not imported.

Three additional builtins (`chr`, `i32_to_i64`, `i64_to_i32`) are internal to
the standard library. User code accesses their functionality through the `conv`
package (`conv.byte_to_str`, `conv.to_i64`, `conv.to_i32`).

## Standard Library Surface

- Embedded stdlib packages currently include `strings`, `utf8`, `conv`, `sort`,
  `path`, `fs`, `process`, `env`, and `stdio`.
- `sort` currently still provides in-place ascending helpers:
  `strings([]str)`, `i32s([]i32)`, and `i64s([]i64)`.
- `path` is pure yar code and provides `clean`, `join`, `dir`, `base`, and
  `ext` for host-style path manipulation.
- `fs` provides explicit-error text file and directory operations: `read_file`,
  `write_file`, `read_dir`, `stat`, `mkdir_all`, `remove_all`, and `temp_dir`.
- `fs.read_dir` returns `[]fs.DirEntry`, where `DirEntry` has `name str` and
  `is_dir bool`.
- `fs.stat` returns `!fs.EntryKind`, where `EntryKind` cases are `File`,
  `Directory`, and `Other`.
- Host filesystem failures surface through ordinary `error` values using stable
  names: `NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidPath`, and
  `IO`.
- `process.args()` returns `[]str`, `process.run([]str)` returns
  `!process.Result`, and `process.run_inherit([]str)` returns `!i32`.
- `process.Result` has `exit_code i32`, `stdout str`, and `stderr str`.
- `env.lookup(str)` returns `!str`.
- `stdio.eprint(str)` writes to stderr and returns `void`.
- Host process/environment failures surface through ordinary `error` values
  using stable names: `NotFound`, `PermissionDenied`, `InvalidArgument`, and
  `IO`.
