package url

pub error InvalidEscape
pub error NotFound

pub struct Param {
    pub name str
    pub value str
}

pub struct Query {
    params []Param
}

pub fn percent_decode(s str) !str {
    return decode(s, false)
}

pub fn percent_encode(s str) str {
    sb := sb_new()
    for i := 0; i < len(s); i += 1 {
        byte := s[i]
        if unreserved_byte(byte) {
            sb_write(sb, s[i:i + 1])
        } else {
            sb_write(sb, "%")
            sb_write(sb, upper_hex(byte / 16))
            sb_write(sb, upper_hex(byte % 16))
        }
    }
    return sb_finish(sb)
}

pub fn parse_query(raw str) !Query {
    params := []Param{}
    start := 0
    for i := 0; i <= len(raw); i += 1 {
        if i < len(raw) && raw[i] != '&' {
            continue
        }
        if i > start {
            params = append(params, parse_param(raw[start:i])?)
        }
        start = i + 1
    }
    return Query{params: params}
}

pub fn (q Query) get(name str) !str {
    for i := 0; i < len(q.params); i += 1 {
        if q.params[i].name == name {
            return q.params[i].value
        }
    }
    return error.NotFound
}

pub fn (q Query) values(name str) []str {
    values := []str{}
    for i := 0; i < len(q.params); i += 1 {
        if q.params[i].name == name {
            values = append(values, q.params[i].value)
        }
    }
    return values
}

pub fn (q Query) params() []Param {
    params := []Param{}
    for i := 0; i < len(q.params); i += 1 {
        params = append(params, q.params[i])
    }
    return params
}

fn parse_param(pair str) !Param {
    for i := 0; i < len(pair); i += 1 {
        if pair[i] == '=' {
            return Param{
                name: decode(pair[0:i], true)?,
                value: decode(pair[i + 1:], true)?,
            }
        }
    }
    return Param{name: decode(pair, true)?, value: ""}
}

fn decode(s str, plus_is_space bool) !str {
    sb := sb_new()
    i := 0
    for i < len(s) {
        byte := s[i]
        if byte == '%' {
            if i + 2 >= len(s) {
                sb_discard(sb)
                return error.InvalidEscape
            }
            high := hex_value(s[i + 1])
            low := hex_value(s[i + 2])
            if high < 0 || low < 0 {
                sb_discard(sb)
                return error.InvalidEscape
            }
            sb_write(sb, chr(high * 16 + low))
            i += 3
        } else if byte == '+' && plus_is_space {
            sb_write(sb, " ")
            i += 1
        } else {
            sb_write(sb, s[i:i + 1])
            i += 1
        }
    }
    return sb_finish(sb)
}

fn unreserved_byte(byte i32) bool {
    return (byte >= 'A' && byte <= 'Z') ||
        (byte >= 'a' && byte <= 'z') ||
        (byte >= '0' && byte <= '9') ||
        byte == '-' ||
        byte == '.' ||
        byte == '_' ||
        byte == '~'
}

fn hex_value(byte i32) i32 {
    if byte >= '0' && byte <= '9' {
        return byte - '0'
    }
    if byte >= 'a' && byte <= 'f' {
        return byte - 'a' + 10
    }
    if byte >= 'A' && byte <= 'F' {
        return byte - 'A' + 10
    }
    return -1
}

fn upper_hex(value i32) str {
    return "0123456789ABCDEF"[value:value + 1]
}
