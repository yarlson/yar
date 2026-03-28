package main

fn gap_small(a i64, b i64) bool {
    return (b - a) <= 10
}

fn main() i32 {
    ok := gap_small(5000000000, 5000000009)
    if ok {
        return 0
    }
    return 1
}
