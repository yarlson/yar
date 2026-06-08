package path

import "conv"
import "strings"

fn normalize_separators(p str) str {
    result := ""
    i := 0
    for i < len(p) {
        b := p[i]
        if b == 92 {
            result = result + "/"
        } else {
            result = result + conv.byte_to_str(b)
        }
        i = i + 1
    }
    return result
}

fn prefix_len(p str) i32 {
    if len(p) >= 3 && p[1] == 58 && p[2] == 47 {
        return 3
    }
    if len(p) >= 2 && p[1] == 58 {
        return 2
    }
    if len(p) > 0 && p[0] == 47 {
        return 1
    }
    return 0
}

pub fn clean(p str) str {
    raw := normalize_separators(p)
    if len(raw) == 0 {
        return "."
    }

    start := prefix_len(raw)
    prefix := raw[0:start]
    parts := []str{}
    i := start
    for i <= len(raw) {
        j := i
        for j < len(raw) && raw[j] != 47 {
            j = j + 1
        }
        if j > i {
            part := raw[i:j]
            if part == "." {
            } else if part == ".." {
                if len(parts) > 0 && parts[len(parts) - 1] != ".." {
                    parts = parts[0:len(parts) - 1]
                } else if start == 0 {
                    parts = append(parts, part)
                }
            } else {
                parts = append(parts, part)
            }
        }
        i = j + 1
    }

    if len(parts) == 0 {
        if len(prefix) > 0 {
            return prefix
        }
        return "."
    }

    result := prefix
    if len(result) > 0 && result[len(result) - 1] != 47 {
        result = result + "/"
    }
    return result + strings.join(parts, "/")
}

pub fn join(parts []str) str {
    result := ""
    i := 0
    for i < len(parts) {
        part := parts[i]
        if len(part) > 0 {
            if len(result) == 0 {
                result = part
            } else {
                result = result + "/" + part
            }
        }
        i = i + 1
    }
    return clean(result)
}

pub fn dir(p str) str {
    cleaned := clean(p)
    if cleaned == "." {
        return "."
    }
    if cleaned == "/" {
        return "/"
    }
    if len(cleaned) == 3 && cleaned[1] == 58 && cleaned[2] == 47 {
        return cleaned
    }
    i := len(cleaned) - 1
    for i >= 0 {
        if cleaned[i] == 47 {
            if i == 0 {
                return "/"
            }
            return cleaned[0:i]
        }
        i = i - 1
    }
    if len(cleaned) >= 2 && cleaned[1] == 58 {
        return cleaned[0:2]
    }
    return "."
}

pub fn base(p str) str {
    cleaned := clean(p)
    if cleaned == "." || cleaned == "/" {
        return cleaned
    }
    i := len(cleaned) - 1
    for i >= 0 {
        if cleaned[i] == 47 {
            return cleaned[i + 1:len(cleaned)]
        }
        i = i - 1
    }
    return cleaned
}

pub fn ext(p str) str {
    b := base(p)
    i := len(b) - 1
    for i >= 0 {
        if b[i] == 46 {
            if i == 0 {
                return ""
            }
            return b[i:len(b)]
        }
        i = i - 1
    }
    return ""
}
