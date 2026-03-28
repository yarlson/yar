# Proposal: Enums / Tagged Unions

Status: accepted

## 1. Summary

Add a closed sum-type construct that covers both plain enums and payload-carrying
tagged unions.

The proposed surface is:

- plain enum cases for symbolic alternatives
- payload cases for structured variants
- explicit construction syntax
- exhaustive statement-form `match` over enum values

## 2. Motivation

Structs model product types well, but YAR still lacks a direct way to model
disjoint alternatives.

That gap shows up in two important places:

- token kinds want a plain closed enum
- AST nodes want tagged unions with case-specific payloads

Today, users would need to simulate these with structs, sentinel fields, and
manual invariants. That is awkward, error-prone, and especially painful for a
self-hosted frontend.

## 3. User-Facing Examples

### Valid examples

```yar
enum TokenKind {
    Ident
    Int
    String
    If
}

enum Expr {
    Int { value i64 }
    Name { text str }
    Binary {
        left *Expr
        op TokenKind
        right *Expr
    }
}

fn eval_kind(kind TokenKind) i32 {
    match kind {
    case TokenKind.Ident {
        return 1
    }
    case TokenKind.Int {
        return 2
    }
    case TokenKind.String {
        return 3
    }
    case TokenKind.If {
        return 4
    }
    }
}

fn is_name(expr Expr) bool {
    match expr {
    case Expr.Name(v) {
        return v.text == "main"
    }
    case Expr.Int(_) {
        return false
    }
    case Expr.Binary(_) {
        return false
    }
    }
}
```

### Invalid examples

```yar
enum Bad {
    A
    A
}
```

Invalid because enum case names must be unique within the enum.

```yar
match kind {
case TokenKind.Ident {
    return 1
}
}
```

Invalid because `match` on an enum must be exhaustive in the first version.

```yar
case Expr.Binary(v) {
    return v.value
}
```

Invalid when `Binary` payload has no field `value`.

## 4. Semantics

An enum is a closed set of named variants.

- a plain case carries no payload
- a payload case carries a fixed record of named fields
- values are constructed explicitly from enum cases
- the set of cases is fixed by the enum declaration

Examples:

- `TokenKind.If`
- `Expr.Int{value: 1}`
- `Expr.Binary{left: a, op: TokenKind.If, right: b}`

`match` performs case analysis over an enum value.

- each case arm selects one variant
- payload cases may bind the payload to a local name
- plain cases have no payload binding
- the first version supports statement-form `match` only
- the first version requires exhaustiveness
- no fallthrough is allowed

This proposal intentionally scopes `match` narrowly to enum case analysis. It is
not intended to introduce general-purpose pattern matching over arbitrary data.

## 5. Type Rules

- enum case names must be unique within their enum
- payload field names must be unique within a payload case
- `Enum.Case` is a value of type `Enum` for plain cases
- `Enum.Case{...}` is a value of type `Enum` for payload cases when field values
  match the declared payload field types
- `match x { ... }` requires `x` to be an enum value
- all enum cases must be covered in a `match`
- a payload binding inside `case Enum.Case(v)` has the generated payload struct
  type for that case
- `_` ignores a payload binding

## 6. Grammar / Parsing Shape

Add:

- top-level `enum` declarations
- payload case bodies using struct-like field lists
- `match` statements for enum case analysis

Example declaration shape:

```yar
enum Result {
    Ok { value i32 }
    Err { code error }
}
```

Example case analysis shape:

```yar
match result {
case Result.Ok(v) {
    return v.value
}
case Result.Err(v) {
    return 0
}
}
```

## 7. Lowering / Implementation Model

- parser: add enum declarations, case forms, constructors, and `match`
- AST / IR: add enum declarations and match nodes
- checker: validate case uniqueness, payload typing, constructor legality, and
  match exhaustiveness
- codegen: lower enums to tag-plus-payload representations
- runtime: no dedicated runtime support is required if lowering uses ordinary
  data layout and branch control flow

The likely representation is:

- a numeric tag
- storage large enough for the largest payload

`match` then lowers to tag inspection plus case-specific payload projection.

## 8. Interactions

- errors: errors remain their own explicit mechanism and should not be replaced
  by tagged unions as the default error model
- structs: payload cases are structurally close to named structs and should feel
  familiar
- arrays: arrays of enum values should be valid
- control flow: `match` is a new branch form, but only for enum case analysis
- returns: enum values return like other first-class values
- builtins: no builtin changes are required
- future modules/imports: exported enums and case visibility need clear package
  rules
- future richer type features: pointers are important for recursive tagged unions
  such as ASTs

## 9. Alternatives Considered

### Plain enums only, no payloads

Too weak for AST and other structured sum-type use cases.

### Model alternatives with structs and sentinel fields

Possible, but clumsy and too easy to misuse.

### General pattern matching immediately

Rejected for now because it broadens the feature far beyond the immediate need
for closed variant modeling.

## 10. Complexity Cost

- language surface: medium to high
- parser complexity: medium to high
- checker complexity: high
- lowering/codegen complexity: high
- runtime complexity: low
- diagnostics complexity: high
- test burden: high
- documentation burden: high

## 11. Why Now?

Closed variants are one of the main missing data-modeling capabilities between
the earlier language and a self-hosted frontend.

The implemented version keeps scope bounded by choosing statement-form `match`,
mandatory exhaustiveness, and no default arm or general pattern system.

## 12. Open Questions

- Should `match` be an expression, a statement, or both in the first version?
- Should wildcard/default arms exist, or should exhaustive explicit listing be
  required always?
- Should plain enum cases share the same syntax and machinery as payload cases,
  or should there be a split between simple enums and tagged unions?
- What exact payload-binding syntax best fits YAR's readability goals?

## 13. Decision

Accepted and implemented.

The shipped version includes:

- top-level `enum` declarations
- plain cases and payload cases
- plain-case and payload-case construction
- exhaustive statement-form `match`
- payload binding with named locals or `_`

The first implementation intentionally does not include:

- `match` as an expression
- wildcard/default arms
- enum equality
- general pattern matching

## 14. Implementation Checklist

- parser
- AST / IR updates
- checker
- codegen
- diagnostics
- tests
- `current-state.md` update
- `decisions.md` update
