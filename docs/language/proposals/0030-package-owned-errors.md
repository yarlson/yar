# Proposal: Package-Owned Error Declarations and Identity

Status: accepted

Implementation: implemented

## 1. Summary

Require named errors to be declared by a package. A declaration is private by
default and may be exported with `pub`:

```yar
package storage

pub error NotFound
error Corrupt
```

The declaring package writes `error.NotFound` or `error.Corrupt`. An importer
uses its package qualifier for exported errors, such as `storage.NotFound`.
The declaration's origin-safe package identity is part of the error identity.

## 2. Motivation

Previously every spelling of `error.Name` created or reused an ambient
program-wide name. A typo silently created a new error, unrelated packages
that chose the same leaf name compared equal, and imports provided no
visibility boundary. Host-backed errors were registered as raw strings by the
checker and code generator rather than owned by their public packages.

Named errors are API declarations. Giving them the same package ownership and
origin identity as other declarations closes typo-driven creation, supports
private implementation errors, and prevents dependencies from accidentally
sharing identities.

## 3. Syntax

Top-level declarations are:

```yar
error Internal
pub error Public
```

`error.Name` is the local expression form. It resolves only a declaration in
the current package or one of the fixed compiler-owned errors.

```yar
fn decode() !str {
    return error.Internal
}
```

`pkg.Name` is the imported expression form. `pkg` is the import qualifier and
`Name` must be a public error declared by that package.

```yar
import "storage"

error Fallback

fn load() !str {
    value := storage.read() or |err| {
        if err == storage.NotFound {
            return error.Fallback
        }
        err?
    }
    return value
}
```

The example package must declare `Fallback`; writing an undeclared local or
imported error is a compile-time error.

## 4. Visibility and API Boundaries

- `error Name` is nameable only in its declaring package.
- `pub error Name` is also nameable through an import qualifier.
- Imports do not add errors to an ambient namespace.
- A private error may propagate through a public `!T` function. Errorable
  signatures do not declare a closed error set, so rejecting this partially
  would create a misleading API guarantee.
- A caller that receives a private error may propagate it, bind it with
  `or |err|`, compare it with another obtained error value, or pass it to
  `to_str`. The caller cannot construct or directly name the private identity.

Typed error sets and exhaustive error declarations on function signatures are
outside this proposal.

## 5. Identity and Codes

The semantic identity of a declared error is the pair of its origin-safe
`PackageId` and declared name. Lowering assigns the same canonical declaration
identity used to distinguish declarations from entry, path, git, and stdlib
origins.

Consequences:

- repeated references to one declaration compare equal;
- importing one package through another valid path does not create a new
  identity;
- two packages declaring the same leaf name produce distinct values;
- packages with the same source package name but different origins remain
  distinct.

The checker sorts canonical identities and assigns non-zero `i32` codes for one
compiled program. Codes are deterministic for the same compilation graph but
are not stable across different programs or versions and must not be persisted
or treated as a host ABI.

## 6. Comparison, Handling, and Display

Error values remain ordinary values of builtin type `error`. `==` and `!=`
compare semantic identity through the program-local code. Direct return, `?`,
`or |err|`, closures, calls, and task results preserve that code unchanged.

`to_str(err)` intentionally preserves the accepted `"error.Name"` display.
Unhandled-error output likewise uses the declared leaf name. Two distinct
package-owned errors with the same leaf may therefore have the same display
string; display text is not identity and programs must use error comparison
when identity matters. Unknown numeric codes still stringify as
`"error.unknown"`.

## 7. Compiler-Owned Errors

Operations without a declaring source package use two fixed language-owned
declarations:

- `error.MissingKey` for missing map keys;
- `error.Closed` for closed channels and closed or invalid resource handles.

These names are always available and cannot be redeclared. Filesystem and
network closed-handle statuses intentionally use the shared `error.Closed`
identity.

## 8. Standard Library and Host Errors

All other standard-library errors are declared by their owning package and are
public when callers need to identify them:

- `fs`: `AlreadyExists`, `IO`, `InvalidArgument`, `InvalidPath`, `NotFound`,
  `PermissionDenied`
- `net`: `AddrInUse`, `ConnectionRefused`, `ConnectionReset`, `IO`,
  `InvalidArgument`, `NotFound`, `PermissionDenied`, `Timeout`
- `process`: `Cancelled`, `IO`, `InvalidArgument`, `LimitExceeded`, `NotFound`,
  `PermissionDenied`, `Timeout`
- `env`: `IO`, `InvalidArgument`, `NotFound`, `PermissionDenied`
- `io`: `IO`, `InvalidArgument`, `LimitExceeded`
- `strings`: `IntegerOverflow`, `InvalidInteger`
- `utf8`: `InvalidUTF8`, `OutOfRange`

Host status conversion maps to these resolved package declarations rather than
registering raw leaf strings. Unknown host statuses use the owning package's
`IO`; closed filesystem and network resources use compiler-owned
`error.Closed`.

## 9. Diagnostics

The compiler reports:

- an undeclared local error at the `error.Name` expression;
- an absent imported error as a missing package member;
- a private imported error as not exported;
- duplicate declarations in one package;
- attempted redeclaration of `Closed` or `MissingKey` as a reserved error.

Diagnostics use source package and leaf names. Internal canonical origin
prefixes are not user-visible.

## 10. Representation and Compatibility

The runtime representation does not change. Plain errors remain `i32` codes.
Errorable results retain their existing flag, code, and optional success-value
layout. Runtime status ABIs and generated result layouts are unchanged; only
the compiler-side mapping from a status to a declared identity changes.

This is a source migration: programs must declare their local errors and use
qualified imported errors. The legacy string form remains stable.

## 11. Alternatives Considered

### Keep ambient names and diagnose only obvious typos

There is no principled way to distinguish a typo from a new ambient error, and
same-name dependencies would still alias accidentally.

### Put all errors in one global standard package

This preserves global coupling and makes unrelated packages coordinate names.
Package ownership keeps APIs local and composes across dependency origins.

### Reject private errors from exported functions

Without declared error sets, direct and transitive propagation cannot be
checked completely. A partial restriction would look stronger than it is.

### Change stringification to a qualified identity

That would improve textual uniqueness but break the accepted `to_str` contract
and expose package naming choices as runtime text. Identity comparison already
provides the correct semantic operation.

## 12. Decision

Accepted. Named errors are package declarations with origin-safe identity,
closed name resolution, and ordinary visibility. Runtime layouts remain
unchanged and display strings preserve the legacy leaf-name format.

## 13. Acceptance and Implementation Checklist

- [x] parse private and public package-level error declarations
- [x] resolve local `error.Name` and imported `pkg.Name` expressions
- [x] reject undeclared, duplicate, reserved, and non-exported errors
- [x] preserve canonical identity across package lowering and source origins
- [x] assign deterministic program-local codes to distinct identities
- [x] preserve equality, propagation, handlers, and legacy `to_str` behavior
- [x] predeclare `error.MissingKey` and `error.Closed`
- [x] migrate stdlib declarations, consumers, and host mappings
- [x] preserve runtime status and errorable-result ABI layouts
- [x] update public, internal, derived, and design documentation
