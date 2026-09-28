package http

import "std/strings"

fn valid_http_version(value str) bool {
    return len(value) == 8 &&
        value[0:5] == "HTTP/" &&
        decimal_byte(value[5]) &&
        value[6] == '.' &&
        decimal_byte(value[7])
}

fn parse_target_authority(method str, target str) !str {
    if len(target) == 0 {
        return error.BadRequest
    }
    if target == "*" {
        if method != "OPTIONS" {
            return error.BadRequest
        }
        return ""
    }
    if target[0] == '/' {
        if !valid_origin_target(target) {
            return error.BadRequest
        }
        return ""
    }
    return parse_absolute_target(target)
}

fn parse_absolute_target(target str) !str {
    colon := strings.index(target, ":")
    if colon <= 0 || !valid_scheme(target[0:colon]) {
        return error.BadRequest
    }
    remainder := target[colon + 1:]
    scheme := strings.to_lower(target[0:colon])
    if !strings.has_prefix(remainder, "//") {
        if scheme == "http" || scheme == "https" {
            return error.BadRequest
        }
        if !valid_path_query(remainder, false) {
            return error.BadRequest
        }
        return ""
    }

    authority_end := len(remainder)
    for i := 2; i < len(remainder); i += 1 {
        if remainder[i] == '/' || remainder[i] == '?' {
            authority_end = i
            break
        }
    }
    authority := remainder[2:authority_end]
    if !valid_authority(authority) || !valid_path_query(remainder[authority_end:], true) {
        return error.BadRequest
    }
    return authority
}

fn target_path(target str) str {
    path := target
    if len(target) > 0 && target[0] != '/' && target != "*" {
        path = absolute_target_path(target)
    }
    query := strings.index(path, "?")
    if query >= 0 {
        return path[0:query]
    }
    return path
}

fn absolute_target_path(target str) str {
    colon := strings.index(target, ":")
    remainder := target[colon + 1:]
    if !strings.has_prefix(remainder, "//") {
        return remainder
    }
    for i := 2; i < len(remainder); i += 1 {
        if remainder[i] == '/' {
            return remainder[i:]
        }
        if remainder[i] == '?' {
            return "/" + remainder[i:]
        }
    }
    return "/"
}

fn target_query(target str) str {
    query := strings.index(target, "?")
    if query < 0 {
        return ""
    }
    return target[query + 1:]
}

fn valid_origin_target(target str) bool {
    return valid_path_query(target, true)
}

fn valid_path_query(value str, allow_double_slash bool) bool {
    if len(value) > 1 && value[0:2] == "//" && !allow_double_slash {
        return false
    }
    in_query := false
    i := 0
    for i < len(value) {
        byte := value[i]
        if byte == '?' {
            in_query = true
            i += 1
        } else if byte == '%' {
            if i + 2 >= len(value) || hex_digit(value[i + 1]) < 0 || hex_digit(value[i + 2]) < 0 {
                return false
            }
            i += 3
        } else {
            allowed := uri_pchar_byte(byte) || byte == '/' || (in_query && byte == '?')
            if !allowed {
                return false
            }
            i += 1
        }
    }
    return true
}

fn valid_scheme(value str) bool {
    if len(value) == 0 || !alpha_byte(value[0]) {
        return false
    }
    for i := 1; i < len(value); i += 1 {
        byte := value[i]
        if !alpha_byte(byte) && !decimal_byte(byte) && byte != '+' && byte != '-' && byte != '.' {
            return false
        }
    }
    return true
}

fn valid_authority(value str) bool {
    if len(value) == 0 || strings.index(value, "@") >= 0 {
        return false
    }
    if value[0] == '[' {
        close := strings.index(value, "]")
        if close <= 1 || !valid_ip_literal(value[1:close]) {
            return false
        }
        if close + 1 == len(value) {
            return true
        }
        return value[close + 1] == ':' && valid_port(value[close + 2:])
    }

    colon := strings.index(value, ":")
    host := value
    if colon >= 0 {
        host = value[0:colon]
        if strings.index(value[colon + 1:], ":") >= 0 || !valid_port(value[colon + 1:]) {
            return false
        }
    }
    return valid_reg_name(host)
}

fn valid_reg_name(value str) bool {
    if len(value) == 0 {
        return false
    }
    i := 0
    for i < len(value) {
        if value[i] == '%' {
            if i + 2 >= len(value) || hex_digit(value[i + 1]) < 0 || hex_digit(value[i + 2]) < 0 {
                return false
            }
            i += 3
        } else {
            if !unreserved_byte(value[i]) && !sub_delim_byte(value[i]) {
                return false
            }
            i += 1
        }
    }
    return true
}

fn valid_ip_literal(value str) bool {
    return valid_ipv6(value) || valid_ipv_future(value)
}

fn valid_ipv6(value str) bool {
    double_colon := strings.index(value, "::")
    if double_colon < 0 {
        return ipv6_group_count(value, true) == 8
    }
    if strings.index(value[double_colon + 2:], "::") >= 0 {
        return false
    }

    left := value[0:double_colon]
    right := value[double_colon + 2:]
    left_count := ipv6_group_count(left, false)
    right_count := ipv6_group_count(right, true)
    return left_count >= 0 && right_count >= 0 && left_count + right_count < 8
}

fn ipv6_group_count(value str, allow_ipv4_tail bool) i32 {
    if len(value) == 0 {
        return 0
    }
    groups := strings.split(value, ":")
    count := 0
    for i := 0; i < len(groups); i += 1 {
        group := groups[i]
        if len(group) == 0 {
            return -1
        }
        if strings.index(group, ".") >= 0 {
            if !allow_ipv4_tail || i + 1 != len(groups) || !valid_ipv4(group) {
                return -1
            }
            count += 2
        } else {
            if len(group) > 4 {
                return -1
            }
            for j := 0; j < len(group); j += 1 {
                if hex_digit(group[j]) < 0 {
                    return -1
                }
            }
            count += 1
        }
    }
    return count
}

fn valid_ipv4(value str) bool {
    parts := strings.split(value, ".")
    if len(parts) != 4 {
        return false
    }
    for i := 0; i < len(parts); i += 1 {
        part := parts[i]
        if len(part) == 0 || len(part) > 3 || (len(part) > 1 && part[0] == '0') {
            return false
        }
        number := 0
        for j := 0; j < len(part); j += 1 {
            if !decimal_byte(part[j]) {
                return false
            }
            number = number * 10 + part[j] - '0'
        }
        if number > 255 {
            return false
        }
    }
    return true
}

fn valid_ipv_future(value str) bool {
    if len(value) < 4 || (value[0] != 'v' && value[0] != 'V') {
        return false
    }
    dot := strings.index(value, ".")
    if dot < 2 || dot + 1 >= len(value) {
        return false
    }
    for i := 1; i < dot; i += 1 {
        if hex_digit(value[i]) < 0 {
            return false
        }
    }
    for i := dot + 1; i < len(value); i += 1 {
        byte := value[i]
        if !unreserved_byte(byte) && !sub_delim_byte(byte) && byte != ':' {
            return false
        }
    }
    return true
}

fn valid_port(value str) bool {
    for i := 0; i < len(value); i += 1 {
        if !decimal_byte(value[i]) {
            return false
        }
    }
    return true
}

fn valid_chunk_extensions(value str) bool {
    i := skip_bws(value, 0)
    for true {
        name_start := i
        for i < len(value) && token_byte(value[i]) {
            i += 1
        }
        if i == name_start {
            return false
        }
        i = skip_bws(value, i)
        if i < len(value) && value[i] == '=' {
            i = skip_bws(value, i + 1)
            if i >= len(value) {
                return false
            }
            if value[i] == '"' {
                i = quoted_string_end(value, i)
                if i < 0 {
                    return false
                }
            } else {
                token_start := i
                for i < len(value) && token_byte(value[i]) {
                    i += 1
                }
                if i == token_start {
                    return false
                }
            }
            i = skip_bws(value, i)
        }
        if i == len(value) {
            return true
        }
        if value[i] != ';' {
            return false
        }
        i = skip_bws(value, i + 1)
    }
    return false
}

fn quoted_string_end(value str, start i32) i32 {
    i := start + 1
    for i < len(value) {
        byte := value[i]
        if byte == '"' {
            return i + 1
        }
        if byte == '\\' {
            i += 1
            if i >= len(value) || !quoted_pair_byte(value[i]) {
                return -1
            }
        } else if !quoted_text_byte(byte) {
            return -1
        }
        i += 1
    }
    return -1
}

fn skip_bws(value str, start i32) i32 {
    i := start
    for i < len(value) && (value[i] == ' ' || value[i] == '\t') {
        i += 1
    }
    return i
}

fn uri_pchar_byte(value i32) bool {
    return unreserved_byte(value) || sub_delim_byte(value) || value == ':' || value == '@'
}

fn unreserved_byte(value i32) bool {
    return alpha_byte(value) || decimal_byte(value) ||
        value == '-' || value == '.' || value == '_' || value == '~'
}

fn sub_delim_byte(value i32) bool {
    return value == '!' || value == '$' || value == '&' || value == '\'' ||
        value == '(' || value == ')' || value == '*' || value == '+' ||
        value == ',' || value == ';' || value == '='
}

fn alpha_byte(value i32) bool {
    return (value >= 'A' && value <= 'Z') || (value >= 'a' && value <= 'z')
}

fn decimal_byte(value i32) bool {
    return value >= '0' && value <= '9'
}

fn quoted_text_byte(value i32) bool {
    return value == '\t' || value == ' ' || value == '!' ||
        (value >= 35 && value <= 91) || (value >= 93 && value <= 126) || value >= 128
}

fn quoted_pair_byte(value i32) bool {
    return value == '\t' || value == ' ' || (value >= 33 && value <= 126) || value >= 128
}
