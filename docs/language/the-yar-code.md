# The Yar Code

These are the articles of the Yar programming language. They describe how Yar
programs are written, how the language works, and why certain things are the way
they are.

When two articles conflict, the lower number wins.

---

**I. Handle every error at the call site.**
Yar has no exceptions. Every function that can fail returns `!T` or `error`.
The caller must `return` the result directly, propagate it with `?`, or handle
it with `or |err| { ... }`. Raw errorable values cannot be stored, passed, or
ignored — the compiler rejects them.

```
content := fs.read_file(path)?
count := strings.count(content, needle) or |err| { return 0 }
```

**II. Write the types out.**
Yar does not infer generic type arguments, does not coerce between named types,
and does not auto-convert between pointer and value receivers. Function
parameters carry explicit types, generic call sites carry explicit type
arguments, and method calls match the receiver exactly.

```
box := Box[i32]{value: 42}
first := first[str](names)
```

**III. Keep imports qualified.**
Imported names stay qualified in every use. There are no wildcard imports, no
aliases, and no injected names. When you read `strings.contains(s, sub)`, you
know where `contains` lives.

```
import "strings"
import "fs"

ok := strings.has_prefix(name, "test_")
data := fs.read_file(name)?
```

**IV. Use `?` for propagation, `or` for recovery.**
`?` propagates an error upward — use it when the caller should decide.
`or |err| { ... }` handles an error locally — use it when you have a fallback.
Do not mix them on the same expression. Pick one intent.

```
// propagate: let the caller deal with it
line := fs.read_file(path)?

// recover: provide a default
line := fs.read_file(path) or |err| { return "" }
```

**V. Model variants with enums and `match`.**
Enums represent closed sets. `match` must cover every case — the compiler
enforces exhaustiveness. Use payload cases to carry data. Use `_` to ignore a
payload you don't need.

```
enum Shape {
    Circle { radius i32 }
    Rect { w i32, h i32 }
}

match shape {
case Shape.Circle(c) {
    print_int(c.radius)
}
case Shape.Rect(r) {
    print_int(r.w * r.h)
}
}
```

**VI. Use pointers for mutation and recursion, not convenience.**
Yar passes structs by value. If a method needs to mutate the receiver, use a
pointer receiver. If a data structure is recursive, use `*T` to break the cycle.
There is no implicit dereference — write `(*ptr).field`, not `ptr.field`.

```
fn (u *User) rename(name str) void {
    (*u).name = name
}

struct Node {
    value i32
    next *Node
}
```

**VII. Prefer slices over arrays for dynamic data.**
Arrays are fixed-size and stack-friendly. Slices are dynamic views over
runtime-managed storage. Use `append` to grow a slice and reassign the result —
`append` may allocate a new backing buffer.

```
items := []str{}
items = append(items, "one")
items = append(items, "two")
part := items[0:1]
```

**VIII. Maps return errors, not zero values.**
Map lookup returns `!V`. A missing key is `error.MissingKey`, not a silent
default. Use `has` to check presence before lookup, or handle the error
directly.

```
if has(counts, key) {
    n := counts[key]?
    print_int(n)
}

n := counts[key] or |err| { return 0 }
```

**IX. Export with `pub`, hide by default.**
Top-level declarations are package-private unless marked `pub`. Exported
declarations cannot leak package-local types through their public surface —
the compiler rejects this.

```
pub struct User {
    id i32
    name str
}

pub fn new_user(id i32, name str) User {
    return User{id: id, name: name}
}
```

**X. Closures capture by value.**
Function literals capture outer locals at creation time, by value. The captured
values are read-only inside the closure. If you need a closure to observe later
changes, pass a pointer explicitly.

```
fn make_adder(base i32) fn(i32) i32 {
    return fn(delta i32) i32 {
        return base + delta
    }
}
```

**XI. Match receivers exactly.**
Value receivers and pointer receivers are distinct. A method declared on `User`
cannot be called on `*User`, and a method declared on `*User` cannot be called
on `User`. The compiler does not insert `&` or `*` for you.

```
fn (u User) display() str {
    return u.name
}

fn (u *User) rename(name str) void {
    (*u).name = name
}
```

**XII. Let `match` replace your `if` chains.**
When branching on an enum value, use `match` — not a chain of `if` comparisons.
`match` is exhaustive, so the compiler tells you when you miss a case. Enum
values do not support `==` or `!=`.

**XIII. Use the stdlib before writing your own.**
The embedded standard library covers strings, UTF-8, conversions, sorting,
paths, filesystem, process execution, environment, and stderr. Stdlib packages
are imported like any other package, and local packages shadow them if you need
to replace one.

```
import "strings"
import "sort"
import "conv"

parts := []str{"c", "a", "b"}
sort.strings(parts)
joined := strings.join(parts, ", ")
```

**XIV. `nil` needs a type.**
`nil` is only valid where the compiler knows the pointer type. You cannot write
`p := nil` because there is no type to infer. Declare the variable with a type,
or use `nil` in a context that already expects a specific pointer.

```
var next *Node = nil
node := Node{value: 1, next: nil}
```
