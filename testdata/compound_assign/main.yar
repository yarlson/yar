package main

fn main() i32 {
    x := 10
    x += 5
    if x != 15 {
        return 1
    }

    x -= 3
    if x != 12 {
        return 2
    }

    x *= 2
    if x != 24 {
        return 3
    }

    x /= 4
    if x != 6 {
        return 4
    }

    x %= 4
    if x != 2 {
        return 5
    }

    // string concatenation
    s := "hello"
    s += " world"
    if s != "hello world" {
        return 6
    }

    // compound assignment in for post clause
    sum := 0
    for i := 0; i < 5; i += 1 {
        sum += i
    }
    if sum != 10 {
        return 7
    }

    print("compound_assign ok\n")
    return 0
}
