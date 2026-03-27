package main

fn add(a i32, b i32) i32 {
    return a + b
}

fn main() i32 {
    x := add(2, 3)
    print_int(x)
    print("\n")
    return 0
}
