package main

import "strings"

fn test_parse_error_empty() bool {
    v := strings.parse_i64("") or |err| {
        return true
    }
    _ := v
    return false
}

fn test_parse_error_bad() bool {
    v := strings.parse_i64("abc") or |err| {
        return true
    }
    _ := v
    return false
}

fn test_parse_error_sign_only() bool {
    v := strings.parse_i64("-") or |err| {
        return true
    }
    _ := v
    return false
}

fn main() !i32 {
    b := strings.from_byte(65)
    if b != "A" {
        return 1
    }
    b2 := strings.from_byte(48)
    if b2 != "0" {
        return 2
    }

    v := strings.parse_i64("12345")?
    if v != 12345 {
        return 3
    }
    v2 := strings.parse_i64("-42")?
    neg42 := i32_to_i64(0 - 42)
    if v2 != neg42 {
        return 4
    }
    v3 := strings.parse_i64("+99")?
    if v3 != 99 {
        return 5
    }
    v4 := strings.parse_i64("0")?
    if v4 != 0 {
        return 6
    }

    if !test_parse_error_empty() {
        return 7
    }
    if !test_parse_error_bad() {
        return 8
    }
    if !test_parse_error_sign_only() {
        return 9
    }

    print("strings_ext ok\n")
    return 0
}
