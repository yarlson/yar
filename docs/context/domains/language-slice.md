# Language Slice

## Source Shape

- A source file starts with a package declaration and is accepted only when the package name is `main`.
- Functions are top-level declarations with positional parameters and an explicit return type.
- Return types may be prefixed with `!` to mark the function as errorable.

## Implemented Types

- `bool`
- `i32`
- `i64`
- `str`
- `void`
- `noreturn`

## Statements

- Block statements delimited by `{` and `}`
- `let` bindings with inferred type from the assigned expression
- Reassignment to an existing local
- `if` with a condition and a single `then` block
- `return`
- Expression statements

## Expressions

- Local identifier lookup
- Integer literals with coercion into `i32` or `i64`
- String literals with `\n`, `\t`, `\\`, and `\"` escapes
- Boolean literals
- Function calls
- Grouping with parentheses
- Binary arithmetic: `+`, `-`, `*`, `/`
- Binary comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`

## Semantic Rules

- `main` must exist and return `i32` or `!i32`.
- Parameters cannot use `void`, `noreturn`, or an unknown type.
- `noreturn` functions cannot also be errorable and cannot contain `return`.
- Non-`void` functions must return on every reachable path.
- `if` conditions must be non-errorable `bool` expressions.
- Arithmetic and relational operators require matching integer operands after literal coercion.
- Equality and inequality are supported for integers and `bool`.
- `error.Name` is only valid as the direct operand of `return` inside an errorable function.
- An errorable call may only be returned directly from a function with the same errorable return type.

## Builtins

- `print(str) void`
- `print_int(i32) void`
- `panic(str) noreturn`
