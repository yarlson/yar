package support

pub interface Labeler {
    label(prefix str) str
}

pub struct Record[T] {
    value T
}

struct namedValue {
    name str
}

fn (v namedValue) label(prefix str) str {
    return prefix + v.name
}

pub fn make_labeler(name str) Labeler {
    return namedValue{name: name}
}

pub fn identity[T](value T) T {
    return value
}
