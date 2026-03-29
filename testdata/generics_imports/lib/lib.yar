package lib

pub struct Box[T] {
    value T
}

pub fn wrap[T](value T) Box[T] {
    return Box[T]{value: value}
}
