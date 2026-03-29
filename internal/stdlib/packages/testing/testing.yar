package testing

import "conv"

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

// Generic assertions -- work for any ==-comparable type.

pub fn equal[V](t *T, got V, want V) void {
    if got != want {
        mark_failed(t)
        add_message(t, "values not equal")
    }
}

pub fn not_equal[V](t *T, got V, want V) void {
    if got == want {
        mark_failed(t)
        add_message(t, "values should differ")
    }
}

// Type-specific assertions with rich "got X, want Y" messages.

pub fn equal_i32(t *T, got i32, want i32) void {
    if got != want {
        mark_failed(t)
        add_message(t, "got " + conv.itoa(got) + ", want " + conv.itoa(want))
    }
}

pub fn equal_i64(t *T, got i64, want i64) void {
    if got != want {
        mark_failed(t)
        add_message(t, "got " + conv.itoa64(got) + ", want " + conv.itoa64(want))
    }
}

pub fn equal_str(t *T, got str, want str) void {
    if got != want {
        mark_failed(t)
        add_message(t, "got \"" + got + "\", want \"" + want + "\"")
    }
}

pub fn equal_bool(t *T, got bool, want bool) void {
    if got != want {
        msg := "got "
        if got {
            msg = msg + "true"
        } else {
            msg = msg + "false"
        }
        msg = msg + ", want "
        if want {
            msg = msg + "true"
        } else {
            msg = msg + "false"
        }
        mark_failed(t)
        add_message(t, msg)
    }
}

pub fn not_equal_i32(t *T, got i32, not_want i32) void {
    if got == not_want {
        mark_failed(t)
        add_message(t, "should not equal " + conv.itoa(not_want))
    }
}

pub fn not_equal_i64(t *T, got i64, not_want i64) void {
    if got == not_want {
        mark_failed(t)
        add_message(t, "should not equal " + conv.itoa64(not_want))
    }
}

pub fn not_equal_str(t *T, got str, not_want str) void {
    if got == not_want {
        mark_failed(t)
        add_message(t, "should not equal \"" + not_want + "\"")
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
