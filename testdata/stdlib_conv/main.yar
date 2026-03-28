package main

import "conv"

fn min_i32() i32 {
    return 0 - 2147483647 - 1
}

fn main() i32 {
    if conv.itoa(0) != "0" {
        return 1
    }
    if conv.itoa(42) != "42" {
        return 2
    }
    if conv.itoa(0 - 1) != "-1" {
        return 3
    }
    if conv.itoa(12345) != "12345" {
        return 4
    }
    if conv.itoa(0 - 999) != "-999" {
        return 5
    }

    if conv.itoa64(0) != "0" {
        return 6
    }
    if conv.itoa64(42) != "42" {
        return 7
    }
    neg1 := conv.to_i64(0 - 1)
    if conv.itoa64(neg1) != "-1" {
        return 8
    }
    if conv.itoa64(5000000000) != "5000000000" {
        return 9
    }
    if conv.itoa64(0 - 5000000000) != "-5000000000" {
        return 10
    }

    if conv.itoa(min_i32()) != "-2147483648" {
        return 11
    }

    print("conv ok\n")
    return 0
}
