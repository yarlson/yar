package main

fn add(a i32, b i32) i32 {
    return a + b
}

fn main() i32 {
    x := add(2, 3)
    print(to_str(x))
    print("\n")
    return 0
}
