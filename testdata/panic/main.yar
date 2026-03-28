package main

fn explode() noreturn {
    panic("boom\n")
}

fn main() i32 {
    explode()
    return 0
}
