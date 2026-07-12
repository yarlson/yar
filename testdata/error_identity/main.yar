package main

import "alpha"
import "beta"

fn main() i32 {
    alpha.fail() or |err| {
        if err != alpha.Same {
            return 1
        }
    }
    beta.fail() or |err| {
        if err != beta.Same {
            return 2
        }
    }
    if alpha.Same == beta.Same {
        return 3
    }
    if to_str(alpha.Same) != "error.Same" || to_str(beta.Same) != "error.Same" {
        return 4
    }
    alpha.hidden() or |err| {
        if to_str(err) != "error.Hidden" {
            return 5
        }
    }
    return 0
}
