package main

import "testing"

fn test_add(t *testing.T) void {
    testing.equal_i32(t, add(2, 3), 5)
    testing.equal_i32(t, add(0, 0), 0)
    testing.equal_i32(t, add(-1, 1), 0)
}

fn test_greet(t *testing.T) void {
    testing.equal_str(t, greet("world"), "hello world")
    testing.equal_str(t, greet(""), "hello ")
}

fn test_divide(t *testing.T) void {
    result := divide(10, 2) or |err| {
        testing.fail(t, "unexpected error from divide(10, 2)")
        return
    }
    testing.equal_i32(t, result, 5)
}

fn test_divide_by_zero(t *testing.T) void {
    result := divide(10, 0) or |err| {
        return
    }
    testing.fail(t, "expected error from divide(10, 0)")
}

fn test_bool_assertions(t *testing.T) void {
    testing.is_true(t, add(1, 1) == 2)
    testing.is_false(t, add(1, 1) == 3)
}
