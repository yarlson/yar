package main

fn main() i32 {
    if 'a' != 97 {
        return 1
    }
    if '\n' != 10 {
        return 2
    }
    if '\t' != 9 {
        return 3
    }
    if '\r' != 13 {
        return 4
    }
    if '\0' != 0 {
        return 5
    }
    if '\\' != 92 {
        return 6
    }
    if '\'' != 39 {
        return 7
    }

    var x i32 = 'z'
    if x != 122 {
        return 8
    }

    print("char literals ok\n")
    return 0
}
