package strings

pub fn contains(s str, substr str) bool {
    if len(substr) == 0 {
        return true
    }
    if len(substr) > len(s) {
        return false
    }
    limit := len(s) - len(substr) + 1
    i := 0
    for i < limit {
        if s[i:i + len(substr)] == substr {
            return true
        }
        i = i + 1
    }
    return false
}

pub fn has_prefix(s str, prefix str) bool {
    if len(prefix) > len(s) {
        return false
    }
    return s[0:len(prefix)] == prefix
}

pub fn has_suffix(s str, suffix str) bool {
    if len(suffix) > len(s) {
        return false
    }
    return s[len(s) - len(suffix):len(s)] == suffix
}

pub fn index(s str, substr str) i32 {
    if len(substr) == 0 {
        return 0
    }
    if len(substr) > len(s) {
        return 0 - 1
    }
    limit := len(s) - len(substr) + 1
    i := 0
    for i < limit {
        if s[i:i + len(substr)] == substr {
            return i
        }
        i = i + 1
    }
    return 0 - 1
}

pub fn count(s str, substr str) i32 {
    if len(substr) == 0 {
        return len(s) + 1
    }
    n := 0
    i := 0
    for i <= len(s) - len(substr) {
        if s[i:i + len(substr)] == substr {
            n = n + 1
            i = i + len(substr)
        } else {
            i = i + 1
        }
    }
    return n
}

pub fn repeat(s str, n i32) str {
    if n <= 0 {
        return ""
    }
    result := ""
    i := 0
    for i < n {
        result = result + s
        i = i + 1
    }
    return result
}

pub fn replace(s str, old str, new str, n i32) str {
    if len(old) == 0 {
        return s
    }
    result := ""
    remaining := s
    replaced := 0
    for n < 0 || replaced < n {
        idx := index(remaining, old)
        if idx < 0 {
            break
        }
        result = result + remaining[0:idx] + new
        remaining = remaining[idx + len(old):len(remaining)]
        replaced = replaced + 1
    }
    result = result + remaining
    return result
}

fn contains_byte(cutset str, b i32) bool {
    i := 0
    for i < len(cutset) {
        if cutset[i] == b {
            return true
        }
        i = i + 1
    }
    return false
}

pub fn trim_left(s str, cutset str) str {
    i := 0
    for i < len(s) {
        if !contains_byte(cutset, s[i]) {
            return s[i:len(s)]
        }
        i = i + 1
    }
    return ""
}

pub fn trim_right(s str, cutset str) str {
    i := len(s) - 1
    for i >= 0 {
        if !contains_byte(cutset, s[i]) {
            return s[0:i + 1]
        }
        i = i - 1
    }
    return ""
}

pub fn join(parts []str, sep str) str {
    if len(parts) == 0 {
        return ""
    }
    result := parts[0]
    i := 1
    for i < len(parts) {
        result = result + sep + parts[i]
        i = i + 1
    }
    return result
}
