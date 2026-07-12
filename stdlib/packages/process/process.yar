package process

pub struct Result {
    exit_code i32
    stdout str
    stderr str
}

pub struct Limits {
    timeout_milliseconds i64
    max_stdout_bytes i64
    max_stderr_bytes i64
}

pub struct Cancellation {
    signal chan[bool]
}

pub fn args() []str {
    panic("process.args intrinsic")
}

pub fn limits(timeout_milliseconds i64, max_stdout_bytes i64, max_stderr_bytes i64) !Limits {
    if timeout_milliseconds <= 0 || timeout_milliseconds > 86400000 ||
        max_stdout_bytes < 0 || max_stdout_bytes > 67108864 ||
        max_stderr_bytes < 0 || max_stderr_bytes > 67108864 {
        return error.InvalidArgument
    }
    return Limits{
        timeout_milliseconds: timeout_milliseconds,
        max_stdout_bytes: max_stdout_bytes,
        max_stderr_bytes: max_stderr_bytes,
    }
}

pub fn cancellation() Cancellation {
    return Cancellation{signal: chan_new[bool](1)}
}

pub fn cancel(value Cancellation) void {
    chan_close(value.signal)
}

pub fn run(argv []str, limits Limits, cancellation Cancellation) !Result {
    panic("process.run intrinsic")
}

pub fn run_inherit(argv []str, timeout_milliseconds i64, cancellation Cancellation) !i32 {
    panic("process.run_inherit intrinsic")
}
