# Language Slice

## Source Shape

- A source file starts with a package declaration.
- Entry builds require a `package main` root package with `main` returning `i32` or `!i32`.
- A package is one or more files in one directory that declare the same package name.
- Imports are explicit `import "path"` declarations immediately after the package clause.
- Top-level declarations may be `struct` or `fn`, optionally prefixed with `pub`.
- Functions have positional parameters and an explicit return type.
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
- user-defined struct types
- fixed array types
- slice types

## Statements

- Block statements delimited by `{` and `}`
- `:=` bindings with inferred type from the assigned expression
- `var name Type`
- `var name Type = expr`
- Reassignment to an existing local, struct field, array index, or slice index
- `if`, `else`, and `else if`
- `for cond { ... }`
- `for init; cond; post { ... }`
- `break`
- `continue`
- `return`
- Expression statements

## Expressions

- Local identifier lookup
- Package-qualified function calls such as `lexer.classify()`
- Integer literals with coercion into `i32` or `i64`
- String literals with `\n`, `\t`, `\\`, and `\"` escapes
- Boolean literals
- `error.Name` literals in return position
- Struct literals
- Array literals
- Slice literals
- Function calls
- Grouping with parentheses
- Field access
- Indexing
- Slicing with `s[i:j]`
- Postfix error propagation with `expr?`
- Local error handling with `expr or |err| { ... }`
- Unary operators: `-`, `!`
- Short-circuit boolean operators: `&&`, `||`
- Binary arithmetic: `+`, `-`, `*`, `/`, `%`
- Binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`

## Semantic Rules

- Entry `main` must exist and return `i32` or `!i32`.
- Imported package references must stay qualified.
- Imported packages expose only `pub` top-level declarations.
- Exported functions and structs cannot expose non-exported local struct types in parameters, returns, or fields.
- Duplicate top-level names are rejected package-wide, including across files.
- Import cycles are rejected.
- Parameters cannot use `void`, `noreturn`, or an unknown type.
- Struct fields, array elements, and slice elements cannot use `void`, `noreturn`, or an unknown type.
- Recursive struct containment is rejected.
- `noreturn` functions cannot also be errorable and cannot contain `return`.
- Plain `error` is a valid parameter or return type for non-`main` functions.
- Non-`void` functions must return on every reachable path.
- `if` and `for` conditions must be non-errorable `bool` expressions.
- Arithmetic and relational operators require matching integer operands after literal coercion.
- Equality and inequality are supported for integers and `bool`.
- `&&` and `||` require `bool` operands and evaluate the right operand only when needed.
- Unary `-` requires an integer operand.
- Unary `!` requires a `bool` operand.
- Field access requires a struct value and a known field.
- Indexing requires an array or slice value and an integer index.
- Slicing requires a slice value and integer bounds.
- Slice fields do not count as recursive inline containment, so recursive shapes such as `[]Node` fields are allowed.
- Out-of-range slice indexing and slicing trap at runtime.
- `len` requires an array or slice argument and returns `i32`.
- `append` requires `append([]T, T)` and returns `[]T`.
- `error.Name` is only valid as the direct operand of `return` inside an errorable function or a function returning `error`.
- A raw errorable call cannot be used directly as a value; it must be returned directly, propagated with `?`, or handled with `or |err| { ... }`.
- `?` is only valid on `!T` or `error` expressions and only inside a function that can return an error.
- `or |err| { ... }` is only valid on `!T` or `error` expressions.
- The handler name in `or |err| { ... }` is scoped only to the handler block.
- When `or` is used on a value-producing `!T` expression, the handler block must terminate control flow.

## Builtins

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
- `len([N]T | []T) i32`
- `append([]T, T) []T`

Builtins remain globally available and are not imported.
