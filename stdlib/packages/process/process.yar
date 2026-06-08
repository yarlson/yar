package process

pub struct Result {
    exit_code i32
    stdout str
    stderr str
}

pub fn args() []str {
    panic("process.args intrinsic")
}

pub fn run(argv []str) !Result {
    panic("process.run intrinsic")
}

pub fn run_inherit(argv []str) !i32 {
    panic("process.run_inherit intrinsic")
}
