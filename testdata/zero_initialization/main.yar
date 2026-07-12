package main

interface Reader {
    read() i32
}

struct State {
    count i32
    label str
}

struct Pair {
    left i32
    right i32
}

fn accept_reader(reader Reader) i32 {
    return 0
}

fn receive_closed(channel chan[i32]) i32 {
    value := chan_recv(channel) or |err| {
        if err != error.Closed {
            return 1
        }
        return 0
    }
    return value + 2
}

fn main() i32 {
    var flag bool
    var number i32
    var wide i64
    var text str
    var pointer *i32
    var values []i32
    var reader Reader
    var channel chan[i32]
    var state State
    var empty [0]map[str]i32

    pair := Pair{left: 1}
    numbers := [3]i32{1}
    lookup := map[str]i32{}

    chan_send(channel, 1) or |err| {
        if err != error.Closed {
            return 1
        }
    }
    chan_close(channel)

    if flag || number != 0 || wide != 0 || text != "" || pointer != nil ||
        len(values) != 0 || receive_closed(channel) != 0 ||
        state.count != 0 || state.label != "" || len(empty) != 0 ||
        pair.left != 1 || pair.right != 0 || numbers[0] != 1 ||
        numbers[1] != 0 || numbers[2] != 0 || len(lookup) != 0 ||
        accept_reader(reader) != 0 {
        return 2
    }
    return 0
}
