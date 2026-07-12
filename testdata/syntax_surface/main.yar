package main

import "support"

// This fixture is the compact compatibility surface for syntax tooling.
pub struct Point[T] {
    x T
    y T
}

interface ValueReader {
    read() i32
}

pub enum Outcome {
    Value { number i32 }
    Empty
    Résumé
}

pub fn (p Point[i32]) read() i32 {
    return p.x
}

fn add(left i32, right i32) i32 {
    return left + right
}

fn generic_identity[T](value T) T {
    return value
}

fn maybe(value i32) !i32 {
    if value < 0 {
        return error.Negative
    }
    return value
}

fn named_error() error {
    return error.SyntaxSurface
}

fn no_result() void {
    return
}

fn expression_surface(pointer *i32, reader ValueReader) !i32 {
    *pointer = *pointer + reader.read()
    checked := maybe(*pointer)?
    handled := maybe(checked) or |err| {
        print(to_str(err))
        return 0
    }
    return handled
}

fn control_flow_surface() i32 {
    var total i32
    var initialized i32 = 1
    total = initialized
    total += 2
    total -= 1
    total *= 3
    total /= 2
    total %= 4

    if total == 3 && initialized != 0 || false {
        total = total + 1
    } else if total <= 0 {
        total = -total
    } else {
        total = total - 1
    }

    for {
        break
    }
    for total > 10 {
        total = total - 1
    }
    for index := 0; index < 3; index += 1 {
        if index == 1 {
            continue
        }
        total += index
    }

    {
        nested := true
        if !nested {
            return 1
        }
    }
    return total
}

fn literal_surface() i32 {
    var wide i64 = 9
    var enabled bool = true
    var label str = "typed"
    text := "line\n\t\r\0\\\""
    chars := []i32{'a', '\n', '\t', '\r', '\0', '\\', '\''}
    fixed := [3]i32{1, 2, 3}
    numbers := []i32{0, 1, 2,}
    table := map[str]i32{"one": 1, "two": 2,}
    var queue chan[i32] = chan_new[i32](1)
    chan_close(queue)

    prefix := numbers[:2]
    suffix := numbers[1:]
    middle := numbers[1:2]
    grouped := (fixed[0] + prefix[0]) * suffix[0] / 1 % 5
    one := table["one"] or |err| {
        print(to_str(err))
        return 1
    }
    valid := grouped >= 0 && grouped < 10 && one == 1 && chars[0] == 97
    if valid && wide == 9 && enabled && label == "typed" {
        print(text[0:0])
        return middle[0]
    }
    return 1
}

fn declaration_surface() i32 {
    point := Point[i32]{x: 2, y: 3}
    reader := point
    record := support.Record[i32]{value: 4}
    value := generic_identity[i32](support.identity[i32](record.value))
    pointer := &value
    result := expression_surface(pointer, reader) or |err| {
        print(to_str(err))
        return 1
    }
    labeler := support.make_labeler("yar")
    print(labeler.label("syntax "))
    return result
}

fn match_surface(outcome Outcome) i32 {
    match outcome {
    case Outcome.Value(payload) {
        return payload.number
    }
    case Outcome.Empty {
        return 0
    }
    else {
        return 1
    }
    }
}

fn closure_surface() i32 {
    offset := 2
    var apply fn(i32) i32 = fn(value i32) i32 {
        return value + offset
    }
    return apply(3)
}

fn main() i32 {
    no_result()
    cafe := 1
    tasks := taskgroup []i32 {
        spawn add(cafe, 1)
        spawn closure_surface()
    }
    if tasks[0] != 2 || tasks[1] != 5 {
        return 1
    }
    if control_flow_surface() < 0 {
        return 2
    }
    if literal_surface() != 1 {
        return 3
    }
    if declaration_surface() != 6 {
        return 4
    }
    if match_surface(Outcome.Value{number: 7}) != 7 {
        return 5
    }
    if match_surface(Outcome.Value(8)) != 8 {
        return 6
    }
    if named_error() != error.SyntaxSurface {
        return 7
    }
    if nil == &cafe {
        return 8
    }
    print(" surface ok\n")
    return 0
}
