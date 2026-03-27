# Error Model

## Errorable Returns

- A function declared as `!T` returns either a success value of type `T` or an error code.
- `!void` is valid and carries only the error flag and error code.
- The checker records each distinct `error.Name` returned anywhere in the program.

## Handling Rules

- Returning `error.Name` is only valid inside an errorable function.
- A plain use of an errorable expression is rejected in:
  - `let` bindings
  - assignments
  - function arguments
  - binary operators
  - `if` conditions
  - non-propagating returns
- A call to an errorable function may be returned directly from a function with the same errorable result type.

## Generated Representation

- Each errorable return is lowered to a generated LLVM struct.
- The first field is an error flag.
- The second field is the numeric error code.
- Non-`void` results include a third field for the success value.

## Program Exit Behavior

- When user `main` is non-errorable, the generated native `main` returns its `i32` value directly.
- When user `main` is errorable, the generated wrapper switches on the error code.
- Known error codes print `unhandled error: <Name>` followed by a newline and exit with code `1`.
- If a wrapper sees an unknown code, it prints `unhandled error` followed by a newline and exits with code `1`.
- `panic(str)` writes the message to stderr, flushes stderr, and exits with code `1`.
