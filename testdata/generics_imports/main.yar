package main

import "lib"

fn main() i32 {
    box := lib.wrap[str]("hello")
    print(box.value)
    print("\n")
    return 0
}
