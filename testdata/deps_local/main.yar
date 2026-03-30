package main

import "mathlib"

fn main() i32 {
    result := mathlib.add(3, 4)
    product := mathlib.multiply(5, 6)
    print(to_str(result))
    print("\n")
    print(to_str(product))
    print("\n")
    return 0
}
