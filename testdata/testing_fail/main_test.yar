package main

import "testing"

fn test_pass(t *testing.T) void {
    testing.equal_i32(t, 1, 1)
}

fn test_wrong_sum(t *testing.T) void {
    testing.equal_i32(t, add(2, 2), 5)
}

fn test_wrong_string(t *testing.T) void {
    testing.equal_str(t, "hello", "world")
}
