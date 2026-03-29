package testing

pub struct T {
    name str
    failed bool
    messages []str
}

fn mark_failed(t *T) void {
    (*t).failed = true
}

fn add_message(t *T, msg str) void {
    (*t).messages = append((*t).messages, msg)
}

pub fn (t *T) fail(msg str) void {
    mark_failed(t)
    add_message(t, msg)
}

pub fn (t *T) log(msg str) void {
    add_message(t, msg)
}

pub fn (t *T) has_failed() bool {
    return (*t).failed
}

pub fn equal[V](t *T, got V, want V) void {
    if got != want {
        mark_failed(t)
        add_message(t, "got " + to_str(got) + ", want " + to_str(want))
    }
}

pub fn not_equal[V](t *T, got V, want V) void {
    if got == want {
        mark_failed(t)
        add_message(t, "should not equal " + to_str(want))
    }
}

pub fn is_true(t *T, value bool) void {
    if !value {
        mark_failed(t)
        add_message(t, "expected true, got false")
    }
}

pub fn is_false(t *T, value bool) void {
    if value {
        mark_failed(t)
        add_message(t, "expected false, got true")
    }
}

pub fn fail(t *T, msg str) void {
    mark_failed(t)
    add_message(t, msg)
}
