package main

fn lookup(m map[str]i32, key str) !i32 {
    v := m[key]?
    return v
}

fn main() !i32 {
    counts := map[str]i32{"hello": 1, "world": 2}

    // len
    print(to_str(len(counts)))
    print("\n")

    // has
    if has(counts, "hello") {
        print("has hello\n")
    }
    if !has(counts, "missing") {
        print("no missing\n")
    }

    // lookup with propagation
    v := lookup(counts, "world")?
    print(to_str(v))
    print("\n")

    // assignment
    counts["new"] = 42
    v2 := lookup(counts, "new")?
    print(to_str(v2))
    print("\n")

    // delete
    delete(counts, "hello")
    print(to_str(len(counts)))
    print("\n")

    // i32 key map
    ids := map[i32]str{10: "ten", 20: "twenty"}
    name := ids[10]?
    print(name)
    print("\n")

    // bool key map
    flags := map[bool]i32{true: 1, false: 0}
    ft := flags[true]?
    print(to_str(ft))
    print("\n")

    // missing key handled with or
    val := counts["gone"] or |err| {
        print("caught\n")
        return 0
    }
    print(to_str(val))
    print("\n")
    return 0
}
