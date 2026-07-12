# Error Model

## Core Model

- A function declared as `!T` returns either a success value of type `T` or an
  error code.
- The language also has first-class `!T` value types for constructs that carry
  deferred task results, such as `taskgroup []!T`.
- A function declared as `error` returns an error code directly.
- `!void` is valid and carries only the error flag and error code.
- `error` is a builtin type.
- Packages declare named errors with `error Name` or `pub error Name`.
- Each declaration is identified by its origin-safe package identity and leaf
  name. The checker assigns distinct, deterministic codes within one program.
- Error names are closed: an `error.Name` spelling that does not resolve to a
  declaration is a compile-time error.
- Map indexing returns `!V` and uses `error.MissingKey` when the requested key
  is absent.

## Error Expressions and Comparison

- `error.Name` resolves a declaration in the current package and is valid as a
  general expression that produces a value of type `error`.
- `pkg.Name` resolves a public error in an imported package. Private imported
  errors are not externally nameable.
- `error.Name` is valid as the operand of `return` inside an errorable function
  or a function returning `error`.
- Error values support `==` and `!=` comparison. Both operands must be `error`.
- Equal leaf names declared by different package origins are distinct values.
- Errors are `i32` codes internally; comparison lowers to integer `icmp`.
- `to_str(err)` converts an error value to its `"error.Name"` string
  representation using a generated switch over the program-wide error-code
  table. The legacy leaf-name display is not an identity operation.
- Private errors may escape exported errorable functions. Callers can
  propagate, handle, compare obtained values, and stringify them, but cannot
  name or construct the private declaration.

## Handling Rules

- Returning `error.Name` is valid inside an errorable function or a function
  returning `error`.
- A raw errorable call or lookup expression is rejected in:
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
- First-class `!T` values, such as `taskgroup []!T` elements, may later be
  handled with `?` or `or |err| { ... }`.
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
- `expr?` is rejected inside a taskgroup body at the current function-literal
  depth because propagation would return before the taskgroup join. Errorable
  nested function literals remain independent propagation scopes.
- For `expr or |err| { ... }`, the handler block runs only when the error is
  non-zero.
- In `or`, the bound name has type `error` and is scoped only within the
  handler block.
- For `!T or |err| { ... }`, success yields the `T` value.
- For `error or |err| { ... }`, the construct is valid in statement position
  for local handling.
- For value-producing `!T or |err| { ... }`, the handler block must terminate
  control flow.

## Compiler-Owned and Host-Backed Errors

- `error.MissingKey` and `error.Closed` are fixed compiler-owned declarations.
  They cannot be redeclared. Maps use `MissingKey`; channels and closed or
  invalid resource handles share `Closed`.
- Filesystem intrinsics map host statuses to declarations owned by `fs`:
  `AlreadyExists`, `IO`, `InvalidArgument`, `InvalidPath`, `NotFound`, and
  `PermissionDenied`. Closed resources use compiler-owned `error.Closed`.
- Process intrinsics map to declarations owned by `process`: `Cancelled`,
  `IO`, `InvalidArgument`, `LimitExceeded`, `NotFound`, `PermissionDenied`, and
  `Timeout`.
- Environment intrinsics map to declarations owned by `env`: `IO`,
  `InvalidArgument`, `NotFound`, and `PermissionDenied`.
- Networking intrinsics map to declarations owned by `net`: `AddrInUse`,
  `ConnectionRefused`, `ConnectionReset`, `IO`, `InvalidArgument`, `NotFound`,
  `PermissionDenied`, and `Timeout`. Closed resources use compiler-owned
  `error.Closed`.
- Host mapping uses canonical declarations rather than raw error-name strings.

## Generated Representation

- Each errorable return is lowered to a generated LLVM struct.
- The first field is an error flag.
- The second field is the numeric error code.
- Non-`void` results include a third field for the success value.
- Plain `error` values lower to integer error codes without the result struct
  wrapper.
- Numeric codes are deterministic for one compilation graph but are not a
  stable cross-program or host ABI. Existing result layouts and runtime status
  ABIs are unchanged.

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
  they are currently written to stdout.
- `panic(str)` writes the message to stderr, flushes stderr, and exits with
  code `1`.
