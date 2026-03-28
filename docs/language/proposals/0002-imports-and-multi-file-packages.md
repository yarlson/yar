# Proposal: Imports and Multi-File Packages

Status: accepted

## 1. Summary

Add code organization beyond one-file programs.

The smallest coherent version is:

- multi-file packages
- explicit `import` declarations
- qualified cross-package references
- explicit export markers for top-level declarations

## 2. Motivation

The current one-file restriction keeps YAR small, but it sharply limits program
size, maintainability, and boundary design.

Frontend self-hosting creates direct pressure here. The current compiler is
already organized into focused packages such as tokenization, parsing, checking,
diagnostics, and runtime support. Rewriting that frontend in YAR would require:

- splitting related code across files
- reusing shared types across package boundaries
- keeping internal helpers hidden
- making symbol ownership explicit

Without imports and multi-file packages, larger YAR programs stay impractical.

## 3. User-Facing Examples

### Valid examples

```yar
package token

pub enum Kind {
    Ident
    Int
    String
}
```

```yar
package lexer

import "token"

pub fn classify(kind token.Kind) bool {
    return kind == token.Kind.Ident
}
```

```yar
package main

import "lexer"

fn main() i32 {
    if lexer.classify(lexer.default_kind()) {
        return 0
    }
    return 1
}
```

### Invalid examples

```yar
package lexer

import "token"

fn main() i32 {
    return token.hidden_helper()
}
```

Invalid because only exported declarations are visible outside their package.

```yar
package a

import "b"
```

```yar
package b

import "a"
```

Invalid because package import cycles are not allowed.

```yar
package main

import "token"

fn main() i32 {
    return Kind.Ident
}
```

Invalid because imported names must be package-qualified in the first version.

## 4. Semantics

A package is a set of one or more source files that declare the same package
name.

- files in the same package share one package scope
- declaration order across files does not matter
- imported packages create named package bindings
- top-level declarations are package-local unless marked `pub`
- references across package boundaries are always qualified in the first version

The initial model intentionally avoids aliases, dot imports, wildcard imports,
re-exports, and implicit visibility.

`package main` remains the executable entry package. A program entry build still
requires a `main` package with `main` returning `i32` or `!i32`.

## 5. Type Rules

- package-qualified references such as `token.Kind` and `lexer.classify` are
  valid only when the imported package exports that name
- exported declarations cannot expose package-local types through public
  parameters, returns, or struct fields
- two files in the same package contribute to the same package-level type and
  function namespace
- duplicate top-level names in one package are invalid, even across files
- import cycles are invalid
- importing a package does not automatically place its exported names into the
  local unqualified scope

## 6. Grammar / Parsing Shape

The first version adds:

- `import "path"` declarations after the package clause
- optional `pub` before top-level `struct`, `fn`, and future `enum`

Example file shape:

```yar
package lexer

import "token"
import "diag"

pub fn parse(src str) !i32 {
    return 0
}
```

Qualified references continue to use existing selector syntax.

## 7. Lowering / Implementation Model

- parser: accept import declarations and export markers, and parse more than one
  file into a package unit
- AST / IR: add package/file structure rather than one-file program structure
- checker: add package scopes, import resolution, export visibility, and cycle
  checks
- codegen: lower all declarations in a resolved package graph while preserving
  package-qualified symbol identity
- runtime: no direct changes required

The main implementation cost is not syntax. It is name resolution, package graph
construction, and changing the compiler entry model from one source file to a
package-oriented compilation unit.

## 8. Interactions

- errors: no change to the explicit error model
- structs: exported and package-local structs need clear visibility rules
- arrays: no special interaction
- control flow: no new control-flow surface
- returns: `main` entry rules still apply only to the entry package
- builtins: builtins remain globally available and are not imported
- future modules/imports: this proposal defines the core organization model
- future richer type features: methods, enums, and standard-library design all
  depend on package boundaries being clear

## 9. Alternatives Considered

### Stay one-file longer

Simpler in the short term, but it keeps larger programs and self-hosting out of
reach.

### Add multi-file packages without imports

Useful only inside one package and too limited for real code organization.

### Make all top-level declarations public by default

Rejected because package boundaries should help hide internal helpers rather than
expose everything automatically.

## 10. Complexity Cost

- language surface: medium
- parser complexity: medium
- checker complexity: high
- lowering/codegen complexity: medium to high
- runtime complexity: low
- diagnostics complexity: high
- test burden: high
- documentation burden: high

## 11. Why Now?

This is the main capability gap between toy programs and maintainable systems.
It is also the first prerequisite for any realistic self-hosted frontend.

Even so, the feature remains deferred because its interaction cost is high and
it should not land before name resolution and package semantics are written down
carefully.

## 12. Open Questions

- What exact path syntax should import strings use?
- Should package identity be directory-based, module-based, or something else?
- Should `pub` be the only visibility marker, or is a different export form
  better for YAR?
- How should the CLI move from file-based compilation to package-based builds?

## 13. Decision

Accepted.

The compiler now supports multi-file packages, explicit imports, package-
qualified cross-package references, and `pub` exports for top-level `struct`
and `fn` declarations.

## 14. Implementation Checklist

- parser
- package/file AST model
- checker name resolution and visibility
- import graph and cycle diagnostics
- codegen package compilation changes
- CLI/package build model
- tests
- `current-state.md` update
- `decisions.md` update
