# Language Slice

## Source Shape

- A source file starts with a package declaration and is accepted only when the package name is `main`.
- Functions are top-level declarations with positional parameters and an explicit return type.
- Return types may be prefixed with `!` to mark the function as errorable.
- `let` is not supported; local declarations use `:=`.

## Implemented Types

- `bool`
- `i32`
- `i64`
- `str`
- `void`
- `noreturn`
- `error`

## Statements

- Block statements delimited by `{` and `}`
- `:=` bindings with inferred type from the assigned expression
- Reassignment to an existing local
- `if` with a condition and a single `then` block
- `return`
- Expression statements

## Expressions

- Local identifier lookup
- Integer literals with coercion into `i32` or `i64`
- String literals with `\n`, `\t`, `\\`, and `\"` escapes
- Boolean literals
- `error.Name` literals in return position
- Function calls
- Grouping with parentheses
- Postfix error propagation with `expr?`
- Local error handling with `expr or |err| { ... }`
- Binary arithmetic: `+`, `-`, `*`, `/`
- Binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`

## Semantic Rules

- `main` must exist and return `i32` or `!i32`.
- Parameters cannot use `void`, `noreturn`, or an unknown type.
- `noreturn` functions cannot also be errorable and cannot contain `return`.
- Plain `error` is a valid parameter or return type for non-`main` functions.
- Non-`void` functions must return on every reachable path.
- `if` conditions must be non-errorable `bool` expressions.
- Arithmetic and relational operators require matching integer operands after literal coercion.
- Equality and inequality are supported for integers and `bool`.
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
