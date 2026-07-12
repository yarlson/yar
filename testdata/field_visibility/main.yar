package main

import "model"

fn main() i32 {
    open := model.Open{value: 1}
    open.value += 1
    value := &open.value

    record := model.record[i32](*value, 7)
    record.value += 1
    if record.value != 3 || record.revision() != 7 {
        return 1
    }
    return 0
}
