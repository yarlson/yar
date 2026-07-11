# Terminology

Yar — The compiler CLI and the source language implemented in this repository.

file AST — A parsed source file with a package declaration, optional imports,
and top-level `struct`, `interface`, `enum`, `fn`, and method declarations.

package — A directory of one or more `.yar` files that declare the same package
name and share one top-level namespace.

program — The AST container used for one parsed file and for the combined,
monomorphized declarations produced from an entry package graph.

package graph — The directed acyclic graph of packages rooted at the entry
`package main`, resolved by `crates/yar-compiler` before lowering into one
combined program.

package origin — One source tree that owns package lookup and dependency
bindings: the entry tree, one path dependency, one pinned git source, or the
embedded stdlib.

PackageId — Compiler identity for a package, formed from its package origin and
source-relative subpath. The source import string is not package identity.

canonical name — An origin-safe lowered declaration name derived from a
`PackageId`, used to merge packages into one checked AST without collisions
between equal logical paths from different origins.

type parameter — A declaration-scoped generic type name such as `T` in
`fn first[T](values []T) T`.

type argument — A concrete type supplied at a generic use site such as `i32`
in `first[i32](values)`.

monomorphization — The compiler pass that substitutes explicit type arguments
into generic structs and functions and emits ordinary non-generic declarations
before checking and code generation.

checked program — A monomorphized program paired with the checker metadata
derived from that exact program. It is the semantic frontend result and the
only input accepted by the compiler orchestration layer's LLVM emission step.

unit — The generated LLVM IR result produced from a checked program before
native linking.

diagnostic — A source-positioned parse or semantic problem returned alongside
compilation results instead of as a hard process error.

errorable function — A function declared with `!` before its return type, such
as `!i32` or `!void`.

errorable value type — A first-class `!T` value shape, currently produced by
language constructs such as `taskgroup []!T` results.

error value — A value of builtin type `error`, typically introduced by
returning `error.Name` or by the binder in an `or |err| { ... }` handler.

error code — The integer representation assigned to each distinct returned
`error.Name` value during code generation.

result type — The generated LLVM struct used to represent an errorable return,
carrying an error flag, an error code, and optionally a success value.

propagation sugar — Postfix `?`, which checks an error-producing expression and
returns from the current function when the error is non-nil.

handler sugar — `or |err| { ... }`, which checks an error-producing expression
and runs a local handler block when the error is non-nil.

direct propagation — Returning an errorable call expression unchanged from a
function with the same errorable result type.

builtin — A compiler-owned operation with checker-defined behavior: `print`,
`panic`, `len`, `append`, `has`, `delete`, `keys`, or `to_str`.

host intrinsic — A stdlib declaration whose checker/codegen wiring calls a
runtime helper directly instead of emitted Yar code.

runtime registry handle — A positive, process-local `i64` registry ID for a
runtime-owned string builder, streaming file, TCP listener, or TCP connection.
IDs are kind-checked and never reused within the process.

enum — A user-defined closed variant type with named cases, each case
optionally carrying a payload of named fields.

match — An exhaustive statement that branches on the case of an enum value,
binding payload fields when present.

unhandled error — An errorable `main` result that reaches the generated native
wrapper, which prints an error message and exits with code `1`.

stdlib — The embedded standard library of Yar packages imported through the
compiler-owned `std/...` namespace and compiled through the same pipeline as
user code.

internal builtin — A builtin (`chr`, `i32_to_i64`, `i64_to_i32`) restricted to
stdlib packages and rejected in user code by the package lowerer.

slice — A runtime-managed dynamic sequence type `[]T` backed by a pointer,
length, and capacity descriptor.

taskgroup — A structured-concurrency expression `taskgroup []R { ... }` that
spawns calls, waits for them to finish, and yields a result slice in spawn
order.

spawn — A statement inside a taskgroup body that starts one concurrent call
whose return shape must match the taskgroup element type.

channel — A bounded builtin `chan[T]` value used for FIFO communication
between tasks.

map — A runtime-managed hash table type `map[K]V` with key types restricted to
`bool`, `i32`, `i64`, and `str`.

pub — Export marker for top-level `struct`, `interface`, `enum`, `fn`, and
method declarations, making them visible to importing packages.

method — A top-level function declaration with an explicit receiver such as
`fn (u User) label() str`, callable with `value.label()`.

manifest — The `yar.toml` file declaring the project name and its external
dependencies with alias names and git URLs or local paths. The selected root
manifest's directory anchors project metadata, the root package tree, and
manifest-relative dependency paths.

lock file — The versioned `yar.lock` dependency graph, pinning each reachable
git dependency to an exact commit and content hash and recording full
source/ref child edges.

dependency alias — The short name used in `yar.toml` to refer to an external
dependency, which becomes the top-level import path segment in source code.
Aliases are bindings owned by an importing package origin; `std` is reserved
and cannot be a dependency alias.

dependency index — The package loader's origin-scoped source and alias map,
built from the root manifest, path-dependency manifests, and lock child edges.
Selected locked sources resolve lazily to verified cache directories.
