package main

struct Box[T] {
    value T
}

struct Pair[T, U] {
    first T
    second U
}

fn first[T](values []T) T {
    return values[0]
}

fn wrap[T](value T) Box[T] {
    return Box[T]{value: value}
}

fn main() i32 {
    values := []i32{7, 9}
    box := wrap[i32](first[i32](values))
    pair := Pair[str, i32]{first: "ok", second: 2}

    print(to_str(box.value))
    print("\n")
    print(pair.first)
    print("\n")
    print(to_str(pair.second))
    print("\n")
    return 0
}
