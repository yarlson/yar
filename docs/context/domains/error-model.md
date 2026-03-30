# Error Model

## Core Model

- A function declared as `!T` returns either a success value of type `T` or an
  error code.
- A function declared as `error` returns an error code directly.
- `!void` is valid and carries only the error flag and error code.
- `error` is a builtin type.
- The checker records each distinct `error.Name` returned anywhere in the
  program.
- Map indexing returns `!V` and uses `error.MissingKey` when the requested key
  is absent.

## Error Expressions and Comparison

- `error.Name` is valid as a general expression that produces a value of type
  `error`.
- `error.Name` is valid as the operand of `return` inside an errorable function
  or a function returning `error`.
- Error values support `==` and `!=` comparison. Both operands must be `error`.
- Errors are `i32` codes internally; comparison lowers to integer `icmp`.
- `to_str(err)` converts an error value to its `"error.Name"` string
  representation using a generated switch over the program-wide error-code
  table.

## Handling Rules

- Returning `error.Name` is valid inside an errorable function or a function
  returning `error`.
- A raw `!T` expression is rejected in:
  - `:=` bindings
  - `var` initializers
  - assignments
  - function arguments
  - unary operators
  - binary operators
  - field access
  - indexing
  - `if` and `for` conditions
  - non-propagating returns
  - plain expression statements
- A call to an errorable function may be returned directly from a function with
  the same errorable result type.
- Postfix `?` is propagation sugar only.
- `or |err| { ... }` is local handling sugar only.
- Both sugar forms are compile-time surface features that lower into explicit
  temporaries, checks, branches, and returns.

## Sugar Semantics

- For `expr?`, success yields the underlying success value and error yields an
  immediate return from the current function.
- `expr?` is valid only on `!T` or `error`.
- `expr?` is valid only when the current function can return a compatible error
  shape: either `!T` or plain `error`.
- For `expr or |err| { ... }`, the handler block runs only when the error is
  non-zero.
- In `or`, the bound name has type `error` and is scoped only within the
  handler block.
- For `!T or |err| { ... }`, success yields the `T` value.
- For `error or |err| { ... }`, the construct is valid in statement position
  for local handling.
- For value-producing `!T or |err| { ... }`, the handler block must terminate
  control flow.

## Host-Backed Error Names

- Filesystem intrinsics contribute stable host error names when they are used:
  `AlreadyExists`, `IO`, `InvalidPath`, `NotFound`, and `PermissionDenied`.
- Process and environment intrinsics contribute stable host error names when
  they are used: `IO`, `InvalidArgument`, `NotFound`, and
  `PermissionDenied`.
- Networking intrinsics contribute stable host error names when they are used:
  `AddrInUse`, `Closed`, `ConnectionRefused`, `ConnectionReset`, `IO`,
  `InvalidArgument`, `NotFound`, `PermissionDenied`, and `Timeout`.
- These names join user-declared `error.Name` values in one program-wide
  error-code table.

## Generated Representation

- Each errorable return is lowered to a generated LLVM struct.
- The first field is an error flag.
- The second field is the numeric error code.
- Non-`void` results include a third field for the success value.
- Plain `error` values lower to integer error codes without the result struct
  wrapper.

## Program Exit Behavior

- When user `main` is non-errorable, the generated native `main` returns its
  `i32` value directly.
- When user `main` is errorable, the generated wrapper switches on the error
  code.
- Known error codes print `unhandled error: <Name>` followed by a newline and
  exit with code `1`.
- If the wrapper sees an unknown code, it prints `unhandled error` followed by
  a newline and exits with code `1`.
- Unhandled-error messages are emitted through the runtime `print` path, so
  they currently go to stdout.
- `panic(str)` writes the message to stderr, flushes stderr, and exits with
  code `1`.
