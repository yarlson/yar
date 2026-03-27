package main

fn inner(flag bool) !i32 {
    if flag == true {
        return error.Fail
    }
    return 41
}

fn outer(flag bool) !i32 {
    let x = try inner(flag)
    return x + 1
}

fn main() i32 {
    let x = outer(false) catch {
        return 1
    }
    if x == 42 {
        return 0
    }
    return 2
}
