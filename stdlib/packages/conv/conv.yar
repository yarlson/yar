package conv

import "strings"

pub fn itoa(n i32) str {
    if n == 0 {
        return "0"
    }
    negative := false
    v := n
    if v < 0 {
        negative = true
        v = 0 - v
        if v < 0 {
            return "-2147483648"
        }
    }
    result := ""
    for v > 0 {
        d := v % 10
        result = strings.from_byte(d + 48) + result
        v = v / 10
    }
    if negative {
        result = "-" + result
    }
    return result
}

pub fn to_i64(n i32) i64 {
    return i32_to_i64(n)
}

pub fn to_i32(n i64) i32 {
    return i64_to_i32(n)
}

pub fn byte_to_str(b i32) str {
    return chr(b)
}

pub fn itoa64(n i64) str {
    if n == 0 {
        return "0"
    }
    negative := false
    v := n
    if v < 0 {
        negative = true
        v = 0 - v
        if v < 0 {
            return "-9223372036854775808"
        }
    }
    result := ""
    for v > 0 {
        d := v % 10
        result = strings.from_byte(i64_to_i32(d) + 48) + result
        v = v / 10
    }
    if negative {
        result = "-" + result
    }
    return result
}
