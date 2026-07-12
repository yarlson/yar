package main

interface Reader {
    read() i32
}

fn main() i32 {
    var reader Reader
    return reader.read()
}
