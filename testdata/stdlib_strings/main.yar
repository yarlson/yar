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

    if strings.trim("  hello  ", " ") != "hello" {
        return 15
    }
    if strings.trim("xxhelloxx", "x") != "hello" {
        return 16
    }
    if strings.trim("", " ") != "" {
        return 17
    }

    s1 := strings.split("a.b.c", ".")
    if len(s1) != 3 {
        return 18
    }
    if s1[0] != "a" {
        return 19
    }
    if s1[1] != "b" {
        return 20
    }
    if s1[2] != "c" {
        return 21
    }

    s2 := strings.split("hello", "x")
    if len(s2) != 1 {
        return 22
    }
    if s2[0] != "hello" {
        return 23
    }

    s3 := strings.split("", ".")
    if len(s3) != 1 {
        return 24
    }
    if s3[0] != "" {
        return 25
    }

    s4 := strings.split("abc", "")
    if len(s4) != 3 {
        return 26
    }
    if s4[0] != "a" {
        return 27
    }
    if s4[2] != "c" {
        return 28
    }

    s5 := strings.split("a..b", ".")
    if len(s5) != 3 {
        return 29
    }
    if s5[1] != "" {
        return 30
    }

    if strings.to_lower("Hello World") != "hello world" {
        return 31
    }
    if strings.to_lower("abc123") != "abc123" {
        return 32
    }
    if strings.to_lower("") != "" {
        return 33
    }

    if strings.to_upper("Hello World") != "HELLO WORLD" {
        return 34
    }
    if strings.to_upper("ABC123") != "ABC123" {
        return 35
    }
    if strings.to_upper("") != "" {
        return 36
    }

    print("strings ok\n")
    return 0
}
