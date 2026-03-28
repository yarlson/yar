package main

import "utf8"
import "strings"

fn make2(b0 i32, b1 i32) str {
    return strings.from_byte(b0) + strings.from_byte(b1)
}

fn make3(b0 i32, b1 i32, b2 i32) str {
    return strings.from_byte(b0) + strings.from_byte(b1) + strings.from_byte(b2)
}

fn make4(b0 i32, b1 i32, b2 i32, b3 i32) str {
    return strings.from_byte(b0) + strings.from_byte(b1) + strings.from_byte(b2) + strings.from_byte(b3)
}

fn test_decode_out_of_range() bool {
    v := utf8.decode("a", 5) or |err| {
        return true
    }
    _ := v
    return false
}

fn test_width_negative_offset() bool {
    v := utf8.width("a", 0 - 1) or |err| {
        return true
    }
    _ := v
    return false
}

fn test_decode_truncated() bool {
    truncated := make2(228, 184)
    v := utf8.decode(truncated, 0) or |err| {
        return true
    }
    _ := v
    return false
}

fn main() !i32 {
    src := "hello"
    r := utf8.decode(src, 0)?
    if r != 104 {
        return 1
    }
    w := utf8.width(src, 0)?
    if w != 1 {
        return 2
    }

    if !utf8.is_letter(65) {
        return 3
    }
    if !utf8.is_letter(122) {
        return 4
    }
    if utf8.is_letter(48) {
        return 5
    }
    if !utf8.is_letter(95) {
        return 6
    }

    if !utf8.is_digit(48) {
        return 7
    }
    if !utf8.is_digit(57) {
        return 8
    }
    if utf8.is_digit(65) {
        return 9
    }

    if !utf8.is_space(32) {
        return 10
    }
    if !utf8.is_space(10) {
        return 11
    }
    if !utf8.is_space(9) {
        return 12
    }
    if utf8.is_space(65) {
        return 13
    }

    two_byte := make2(195, 169)
    r3 := utf8.decode(two_byte, 0)?
    if r3 != 233 {
        return 14
    }
    w3 := utf8.width(two_byte, 0)?
    if w3 != 2 {
        return 15
    }

    three_byte := make3(228, 184, 150)
    r4 := utf8.decode(three_byte, 0)?
    if r4 != 19990 {
        return 16
    }
    w4 := utf8.width(three_byte, 0)?
    if w4 != 3 {
        return 17
    }

    four_byte := make4(240, 159, 152, 128)
    r5 := utf8.decode(four_byte, 0)?
    if r5 != 128512 {
        return 18
    }
    w5 := utf8.width(four_byte, 0)?
    if w5 != 4 {
        return 19
    }

    if !test_decode_out_of_range() {
        return 20
    }
    if !test_width_negative_offset() {
        return 21
    }
    if !test_decode_truncated() {
        return 22
    }

    print("utf8 ok\n")
    return 0
}
