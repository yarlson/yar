package main

fn main() i32 {
    var x i64 = 0 - 1
    if x + 1 != 0 {
        return 1
    }

    var y i64 = 2 + 3
    if y != 5 {
        return 2
    }

    z := 0 - 1
    if z != 0 - 1 {
        return 3
    }

    print("i64_untyped_binary ok\n")
    return 0
}
