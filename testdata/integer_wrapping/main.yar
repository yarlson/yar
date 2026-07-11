package main

fn main() i32 {
    max_i32 := 2147483647
    min_i32 := max_i32 + 1
    if min_i32 - 1 != max_i32 {
        return 1
    }
    high_i32 := 1073741824
    if high_i32 * 2 != min_i32 {
        return 2
    }
    if -min_i32 != min_i32 {
        return 3
    }

    var max_i64 i64 = 9223372036854775807
    var one_i64 i64 = 1
    min_i64 := max_i64 + one_i64
    if min_i64 - one_i64 != max_i64 {
        return 4
    }
    var high_i64 i64 = 4611686018427387904
    if high_i64 * 2 != min_i64 {
        return 5
    }
    if -min_i64 != min_i64 {
        return 6
    }

    print("integer wrapping ok\n")
    return 0
}
