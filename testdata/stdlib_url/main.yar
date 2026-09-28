package main

import "std/conv"
import "std/url"

fn main() !i32 {
    if url.percent_decode("a%20b%2Fc%7e")? != "a b/c~" {
        return 1
    }
    if url.percent_decode("a+b")? != "a+b" {
        return 2
    }
    if !invalid_escape("%zz") || !invalid_escape("%4") || !invalid_escape("50%") {
        return 3
    }
    if url.percent_encode("a b/c~-._") != "a%20b%2Fc~-._" {
        return 4
    }
    if url.percent_encode("é&=") != "%C3%A9%26%3D" {
        return 5
    }

    query := url.parse_query("a=1&b=two+words&a=3&&flag&x=%26%3D")?
    if query.get("a")? != "1" || query.get("b")? != "two words" {
        return 6
    }
    if query.get("flag")? != "" || query.get("x")? != "&=" {
        return 7
    }
    values := query.values("a")
    if len(values) != 2 || values[0] != "1" || values[1] != "3" {
        return 8
    }
    if len(query.params()) != 5 || len(query.values("missing")) != 0 {
        return 9
    }
    if !not_found(query, "missing") {
        return 10
    }

    if len(url.parse_query("")?.params()) != 0 {
        return 11
    }
    if !invalid_query("ok=1&bad=%G1") {
        return 12
    }

    print("url ok\n")
    return 0
}

fn invalid_escape(value str) bool {
    decoded := url.percent_decode(value) or |err| {
        return err == url.InvalidEscape
    }
    _ := decoded
    return false
}

fn not_found(query url.Query, name str) bool {
    value := query.get(name) or |err| {
        return err == url.NotFound
    }
    _ := value
    return false
}

fn invalid_query(raw str) bool {
    query := url.parse_query(raw) or |err| {
        return err == url.InvalidEscape
    }
    _ := query
    return false
}
