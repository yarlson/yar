package main

import "std/strings"

error Negative

fn double(value i32) i32 {
    return value * 2
}

fn checked(value i32) !i32 {
    if value < 0 {
        return error.Negative
    }
    return value
}

fn record(log []str, entry str) void {
    log[0] = log[0] + entry
}

fn validate(value i32) !void {
    if value < 0 {
        return error.Negative
    }
}

fn second(_ i32, value i32) i32 {
    return value
}

fn apply(values []i32, transform fn(i32) i32) i32 {
    total := 0
    for i := 0; i < len(values); i += 1 {
        total += transform(values[i])
    }
    return total
}

fn main() !i32 {
    if apply([]i32{1, 2, 3}, double) != 12 {
        return 1
    }

    upper := strings.to_upper
    if upper("yar") != "YAR" {
        return 2
    }

    check := checked
    if check(5)? != 5 {
        return 3
    }
    if !rejects_negative(check) {
        return 4
    }

    log := []str{""}
    write := record
    write(log, "a")
    write(log, "b")
    if log[0] != "ab" {
        return 5
    }

    run := validate
    run(1)?
    rejected := false
    run(-1) or |err| {
        rejected = err == error.Negative
    }
    if !rejected {
        return 6
    }

    double := fn(value i32) i32 {
        return value + 100
    }
    if apply([]i32{1}, double) != 101 {
        return 7
    }

    pick := second
    if pick(1, 8) != 8 {
        return 8
    }

    print("function values ok\n")
    return 0
}

fn rejects_negative(check fn(i32) !i32) bool {
    value := check(-1) or |err| {
        return err == error.Negative
    }
    _ := value
    return false
}
