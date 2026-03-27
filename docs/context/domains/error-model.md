# Error Model

## Core Model

- A function declared as `!T` returns either a success value of type `T` or an error code.
- A function declared as `error` returns an error code directly.
- `!void` is valid and carries only the error flag and error code.
- `error` is a builtin type.
- The checker records each distinct `error.Name` returned anywhere in the program.

## Handling Rules

- Returning `error.Name` is only valid inside an errorable function or a function returning `error`.
- A raw `!T` expression is rejected in:
  - `:=` bindings
  - assignments
  - function arguments
  - binary operators
  - `if` conditions
  - non-propagating returns
- A call to an errorable function may be returned directly from a function with the same errorable result type.
- Postfix `?` is propagation sugar only.
- `or |err| { ... }` is local handling sugar only.
- Both sugar forms are compile-time surface features that lower into explicit temporaries, checks, branches, and returns.

## Sugar Semantics

- For `expr?`, success yields the underlying success value and error yields an immediate return from the current function.
- `expr?` is valid only on `!T` or `error`.
- `expr?` is valid only when the current function can return a compatible error shape: either `!T` or plain `error`.
- For `expr or |err| { ... }`, the handler block runs only when the error is non-zero.
- In `or`, the bound name has type `error` and is scoped only within the handler block.
- For `!T or |err| { ... }`, success yields the `T` value.
- For `error or |err| { ... }`, the construct is valid in statement position for local handling.
- For value-producing `!T or |err| { ... }`, the handler block must terminate control flow.

## Generated Representation

- Each errorable return is lowered to a generated LLVM struct.
- The first field is an error flag.
- The second field is the numeric error code.
- Non-`void` results include a third field for the success value.
- Plain `error` values lower to integer error codes without the result struct wrapper.

## Program Exit Behavior

- When user `main` is non-errorable, the generated native `main` returns its `i32` value directly.
- When user `main` is errorable, the generated wrapper switches on the error code.
- Known error codes print `unhandled error: <Name>` followed by a newline and exit with code `1`.
- If a wrapper sees an unknown code, it prints `unhandled error` followed by a newline and exits with code `1`.
- `panic(str)` writes the message to stderr, flushes stderr, and exits with code `1`.
