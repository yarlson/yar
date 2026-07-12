package model

pub struct Open {
    pub value i32
}

pub struct Record[T] {
    pub value T
    revision i32
}

pub fn record[T](value T, revision i32) Record[T] {
    return Record[T]{value: value, revision: revision}
}

pub fn (record Record[i32]) revision() i32 {
    return record.revision
}
