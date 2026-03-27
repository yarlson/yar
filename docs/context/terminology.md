# Terminology

yar — The compiler CLI and the source language implemented in this repository.

program — A parsed source file with a package declaration and top-level function declarations.

unit — The result of successful compilation before linking; it contains generated LLVM IR and checker metadata.

diagnostic — A source-positioned parse or semantic problem returned alongside compilation results instead of as a hard process error.

errorable function — A function declared with `!` before its return type, such as `!i32` or `!void`.

error code — The integer representation assigned to each distinct returned `error.Name` value during code generation.

result type — The generated LLVM struct used to represent an errorable return, carrying an error flag, an error code, and optionally a success value.

catch — An expression form that unwraps an errorable expression and requires its block to terminate on every path.

try — An expression form that unwraps an errorable expression inside another errorable function and propagates the error code on failure.

builtin — A function signature hard-coded in the checker and lowered specially in code generation: `print`, `print_int`, or `panic`.

unhandled error — An errorable `main` result that reaches the generated native wrapper, which prints an error message and exits with code `1`.
