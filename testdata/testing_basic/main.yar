package main

fn add(a i32, b i32) i32 {
    return a + b
}

fn greet(name str) str {
    return "hello " + name
}

fn divide(a i32, b i32) !i32 {
    if b == 0 {
        return error.DivideByZero
    }
    return a / b
}

fn main() i32 {
    print_int(add(2, 3))
    print("\n")
    return 0
}
