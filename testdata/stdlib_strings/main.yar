package main

import "strings"

fn main() i32 {
    if !strings.contains("hello world", "world") {
        return 1
    }
    if strings.contains("hello", "xyz") {
        return 2
    }

    if !strings.has_prefix("hello world", "hello") {
        return 3
    }
    if strings.has_prefix("hi", "hello") {
        return 4
    }

    if !strings.has_suffix("hello world", "world") {
        return 5
    }
    if strings.has_suffix("hi", "world") {
        return 6
    }

    if strings.index("hello world", "world") != 6 {
        return 7
    }
    if strings.index("hello", "xyz") != 0 - 1 {
        return 8
    }

    if strings.count("abcabc", "abc") != 2 {
        return 9
    }

    if strings.repeat("ab", 3) != "ababab" {
        return 10
    }

    if strings.replace("aabbcc", "bb", "XX", 0 - 1) != "aaXXcc" {
        return 11
    }

    if strings.trim_left("  hello", " ") != "hello" {
        return 12
    }

    if strings.trim_right("hello  ", " ") != "hello" {
        return 13
    }

    parts := []str{"a", "b", "c"}
    if strings.join(parts, "-") != "a-b-c" {
        return 14
    }

    print("strings ok\n")
    return 0
}
