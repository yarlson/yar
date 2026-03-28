# Terminology

yar — The compiler CLI and the source language implemented in this repository.

program — A loaded package graph rooted at an entry `package main`.

file AST — A parsed source file with a package declaration, optional imports,
and top-level `struct` and `fn` declarations.

package — A directory of one or more `.yar` files that declare the same package
name and share one top-level namespace.

unit — The result of successful compilation before linking; it contains generated LLVM IR and checker metadata.

diagnostic — A source-positioned parse or semantic problem returned alongside compilation results instead of as a hard process error.

errorable function — A function declared with `!` before its return type, such as `!i32` or `!void`.

error value — A value of builtin type `error`, typically introduced by returning `error.Name` or by the binder in an `or |err| { ... }` handler.

error code — The integer representation assigned to each distinct returned `error.Name` value during code generation.

result type — The generated LLVM struct used to represent an errorable return, carrying an error flag, an error code, and optionally a success value.

propagation sugar — Postfix `?`, which checks an error-producing expression and returns from the current function when the error is non-nil.

handler sugar — `or |err| { ... }`, which checks an error-producing expression and runs a local handler block when the error is non-nil.

direct propagation — Returning an errorable call expression unchanged from a function with the same errorable result type.

builtin — A compiler-owned operation with checker-defined behavior: `print`, `print_int`, `panic`, `len`, `append`, `has`, or `delete`.

enum — A user-defined closed variant type with named cases, each case optionally carrying a payload of named fields.

match — An exhaustive statement that branches on the case of an enum value, binding payload fields when present.

package graph — The directed acyclic graph of packages rooted at the entry `package main`, resolved by `internal/compiler` before lowering into a single checked program.

unhandled error — An errorable `main` result that reaches the generated native wrapper, which prints an error message and exits with code `1`.

stdlib — The embedded standard library of yar packages (`strings`, `utf8`, `conv`, `sort`, `path`, `fs`, `process`, `env`, `stdio`) compiled through the same pipeline as user code.

internal builtin — A builtin (`chr`, `i32_to_i64`, `i64_to_i32`) restricted to stdlib packages and rejected in user code by the package lowerer.

slice — A runtime-managed dynamic sequence type `[]T` backed by a pointer, length, and capacity descriptor.

map — A runtime-managed hash table type `map[K]V` with key types restricted to `bool`, `i32`, `i64`, and `str`.

pub — Export marker for top-level `struct`, `enum`, and `fn` declarations, making them visible to importing packages.
