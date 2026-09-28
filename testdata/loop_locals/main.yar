package main

import "std/strings"

fn main() !i32 {
    total := 0
    for i := 0; i < 1000000; i += 1 {
        text := to_str(i % 10)
        parts := strings.split(text + ",x", ",")
        total += len(parts[0])
    }
    if total != 1000000 {
        return 1
    }
    print("loop locals ok\n")
    return 0
}
