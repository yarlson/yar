package main

fn main() i32 {
    // string slicing
    s := "hello world"
    if s[6:] != "world" {
        return 1
    }
    if s[:5] != "hello" {
        return 2
    }
    if s[0:] != s {
        return 3
    }
    if s[:len(s)] != s {
        return 4
    }

    // slice slicing
    items := []i32{10, 20, 30, 40, 50}
    tail := items[2:]
    if len(tail) != 3 {
        return 5
    }
    if tail[0] != 30 {
        return 6
    }

    head := items[:3]
    if len(head) != 3 {
        return 7
    }
    if head[2] != 30 {
        return 8
    }

    // full form still works
    mid := items[1:4]
    if len(mid) != 3 {
        return 9
    }
    if mid[0] != 20 {
        return 10
    }

    // string slice
    parts := []str{"a", "b", "c", "d"}
    last_two := parts[2:]
    if len(last_two) != 2 {
        return 11
    }
    if last_two[0] != "c" {
        return 12
    }

    print("open_ended_slice ok\n")
    return 0
}
