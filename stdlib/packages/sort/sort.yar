package sort

fn strings_less(left str, right str) bool {
    shared := len(left)
    if len(right) < shared {
        shared = len(right)
    }

    i := 0
    for i < shared {
        if left[i] < right[i] {
            return true
        }
        if left[i] > right[i] {
            return false
        }
        i = i + 1
    }

    return len(left) < len(right)
}

pub fn strings(values []str) void {
    i := 1
    for i < len(values) {
        current := values[i]
        j := i
        for j > 0 && strings_less(current, values[j - 1]) {
            values[j] = values[j - 1]
            j = j - 1
        }
        values[j] = current
        i = i + 1
    }
}

pub fn i32s(values []i32) void {
    i := 1
    for i < len(values) {
        current := values[i]
        j := i
        for j > 0 && current < values[j - 1] {
            values[j] = values[j - 1]
            j = j - 1
        }
        values[j] = current
        i = i + 1
    }
}

pub fn i64s(values []i64) void {
    i := 1
    for i < len(values) {
        current := values[i]
        j := i
        for j > 0 && current < values[j - 1] {
            values[j] = values[j - 1]
            j = j - 1
        }
        values[j] = current
        i = i + 1
    }
}
