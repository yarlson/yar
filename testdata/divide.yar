package main

fn divide(a i32, b i32) !i32 {
    if b == 0 {
        return error.DivByZero
    }
    return a / b
}

fn main() i32 {
    let x = divide(10, 2) catch {
        print("division failed\n")
        return 1
    }

    print_int(x)
    print("\n")
    return 0
}
