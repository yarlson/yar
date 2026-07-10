package main

fn next_index(counter *i32) i32 {
    *counter += 1
    return 0
}

fn amount(counter *i32) i32 {
    *counter += 1
    return 2
}

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

    index_calls := 0
    rhs_calls := 0
    values := [1]i32{1}
    values[next_index(&index_calls)] += amount(&rhs_calls)
    if index_calls != 1 {
        return 8
    }
    if rhs_calls != 1 {
        return 9
    }
    if values[0] != 3 {
        return 10
    }

    print("compound_assign ok\n")
    return 0
}
