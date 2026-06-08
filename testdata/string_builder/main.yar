package main

fn main() i32 {
    builder := sb_new()
    sb_write(builder, "hello")
    sb_write(builder, " ")
    sb_write(builder, "world")

    result := sb_string(builder)
    if len(result) != 11 {
        return 1
    }
    print(result)
    print("\n")

    empty := sb_string(builder)
    if len(empty) != 0 {
        return 2
    }

    repeated := sb_new()
    var i i32 = 0
    for i < 100 {
        sb_write(repeated, "x")
        i = i + 1
    }

    result2 := sb_string(repeated)
    if len(result2) != 100 {
        return 3
    }

    return 0
}
