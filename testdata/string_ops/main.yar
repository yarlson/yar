package main

fn main() i32 {
    s := "hello world"

    if len(s) != 11 {
        return 1
    }

    if s == "hello world" {
        print("eq ok\n")
    } else {
        return 2
    }

    if s != "other" {
        print("ne ok\n")
    } else {
        return 3
    }

    if s[0] != 104 {
        return 4
    }

    if s[6] != 119 {
        return 5
    }

    sub := s[0:5]
    if sub != "hello" {
        return 6
    }

    cat := "foo" + "bar"
    if cat != "foobar" {
        return 7
    }

    if len(cat) != 6 {
        return 8
    }

    empty := ""
    if len(empty) != 0 {
        return 9
    }

    if empty == "x" {
        return 10
    }

    if empty != "" {
        return 11
    }

    tail := s[6:11]
    if tail != "world" {
        return 12
    }

    print("all ok\n")
    return 0
}
