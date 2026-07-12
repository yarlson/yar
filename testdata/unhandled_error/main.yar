package main

error Boom

fn fail() !i32 {
    return error.Boom
}

fn main() !i32 {
    return fail()
}
