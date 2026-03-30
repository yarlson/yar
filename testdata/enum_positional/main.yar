package main

enum Value {
    Text { val str }
    Number { val i32 }
    Flag { val bool }
    Plain
}

fn check_text(v Value) i32 {
    match v {
        case Value.Text(t) {
            if t.val != "hello" {
                return 1
            }
        }
        else {
            return 2
        }
    }
    return 0
}

fn main() i32 {
    // positional construction for single-field cases
    v1 := Value.Text("hello")
    result := check_text(v1)
    if result != 0 {
        return result
    }

    v2 := Value.Number(42)
    match v2 {
        case Value.Number(n) {
            if n.val != 42 {
                return 3
            }
        }
        else {
            return 4
        }
    }

    v3 := Value.Flag(true)
    match v3 {
        case Value.Flag(f) {
            if !f.val {
                return 5
            }
        }
        else {
            return 6
        }
    }

    // keyed form still works
    v4 := Value.Text{val: "world"}
    match v4 {
        case Value.Text(t) {
            if t.val != "world" {
                return 7
            }
        }
        else {
            return 8
        }
    }

    // plain cases still work
    v5 := Value.Plain
    match v5 {
        case Value.Plain {
            // ok
        }
        else {
            return 9
        }
    }

    print("enum_positional ok\n")
    return 0
}
