package json

import "std/conv"
import "std/strings"
import "std/utf8"

pub error DuplicateName
pub error InvalidJSON
pub error InvalidNumber
pub error InvalidString
pub error NotFound
pub error OutOfRange
pub error TooDeep
pub error TypeMismatch

pub enum Value {
    Null
    Bool { value bool }
    Number { text str }
    String { value str }
    Array { items []Value }
    Object { members []Member }
}

pub struct Member {
    pub name str
    pub value Value
}

struct Parsed {
    value Value
    pos i32
}

struct ParsedString {
    value str
    pos i32
}

pub fn parse(text str) !Value {
    parsed := parse_value(text, skip_space(text, 0), 0)?
    if skip_space(text, parsed.pos) != len(text) {
        return error.InvalidJSON
    }
    return parsed.value
}

pub fn encode(value Value) !str {
    sb := sb_new()
    encode_into(sb, value, 0) or |err| {
        sb_discard(sb)
        err?
        return ""
    }
    return sb_finish(sb)
}

pub fn int(value i64) Value {
    return Value.Number(conv.itoa64(value))
}

pub fn number(text str) !Value {
    if !valid_number(text) {
        return error.InvalidNumber
    }
    return Value.Number(text)
}

pub fn get(value Value, name str) !Value {
    match value {
    case Value.Object(object) {
        for i := 0; i < len(object.members); i += 1 {
            if object.members[i].name == name {
                return object.members[i].value
            }
        }
        return error.NotFound
    }
    else {
        return error.TypeMismatch
    }
    }
}

pub fn is_null(value Value) bool {
    match value {
    case Value.Null {
        return true
    }
    else {
        return false
    }
    }
}

pub fn as_bool(value Value) !bool {
    match value {
    case Value.Bool(b) {
        return b.value
    }
    else {
        return error.TypeMismatch
    }
    }
}

pub fn as_str(value Value) !str {
    match value {
    case Value.String(s) {
        return s.value
    }
    else {
        return error.TypeMismatch
    }
    }
}

pub fn as_i64(value Value) !i64 {
    match value {
    case Value.Number(n) {
        if !valid_number(n.text) || !integer_text(n.text) {
            return error.OutOfRange
        }
        parsed := strings.parse_i64(n.text) or |err| {
            return error.OutOfRange
        }
        return parsed
    }
    else {
        return error.TypeMismatch
    }
    }
}

pub fn as_i32(value Value) !i32 {
    wide := as_i64(value)?
    if wide < conv.to_i64(-2147483647 - 1) || wide > conv.to_i64(2147483647) {
        return error.OutOfRange
    }
    return conv.to_i32(wide)
}

pub fn as_array(value Value) ![]Value {
    match value {
    case Value.Array(a) {
        return a.items
    }
    else {
        return error.TypeMismatch
    }
    }
}

pub fn as_object(value Value) ![]Member {
    match value {
    case Value.Object(o) {
        return o.members
    }
    else {
        return error.TypeMismatch
    }
    }
}

fn parse_value(text str, pos i32, depth i32) !Parsed {
    if pos >= len(text) {
        return error.InvalidJSON
    }
    byte := text[pos]
    if byte == '{' {
        return parse_object(text, pos + 1, depth + 1)
    }
    if byte == '[' {
        return parse_array(text, pos + 1, depth + 1)
    }
    if byte == '"' {
        parsed := parse_string(text, pos + 1)?
        return Parsed{value: Value.String(parsed.value), pos: parsed.pos}
    }
    if byte == '-' || digit(byte) {
        end := number_end(text, pos)
        if end < 0 {
            return error.InvalidJSON
        }
        return Parsed{value: Value.Number(text[pos:end]), pos: end}
    }
    if literal_at(text, pos, "true") {
        return Parsed{value: Value.Bool(true), pos: pos + 4}
    }
    if literal_at(text, pos, "false") {
        return Parsed{value: Value.Bool(false), pos: pos + 5}
    }
    if literal_at(text, pos, "null") {
        return Parsed{value: Value.Null, pos: pos + 4}
    }
    return error.InvalidJSON
}

fn parse_array(text str, start i32, depth i32) !Parsed {
    if depth > max_depth() {
        return error.TooDeep
    }
    items := []Value{}
    pos := skip_space(text, start)
    if pos < len(text) && text[pos] == ']' {
        return Parsed{value: Value.Array(items), pos: pos + 1}
    }
    for pos < len(text) {
        item := parse_value(text, pos, depth)?
        items = append(items, item.value)
        pos = skip_space(text, item.pos)
        if pos < len(text) && text[pos] == ']' {
            return Parsed{value: Value.Array(items), pos: pos + 1}
        }
        if pos >= len(text) || text[pos] != ',' {
            return error.InvalidJSON
        }
        pos = skip_space(text, pos + 1)
    }
    return error.InvalidJSON
}

fn parse_object(text str, start i32, depth i32) !Parsed {
    if depth > max_depth() {
        return error.TooDeep
    }
    members := []Member{}
    names := map[str]bool{}
    pos := skip_space(text, start)
    if pos < len(text) && text[pos] == '}' {
        return Parsed{value: Value.Object(members), pos: pos + 1}
    }
    for pos < len(text) {
        if text[pos] != '"' {
            return error.InvalidJSON
        }
        name := parse_string(text, pos + 1)?
        if has(names, name.value) {
            return error.DuplicateName
        }
        names[name.value] = true
        pos = skip_space(text, name.pos)
        if pos >= len(text) || text[pos] != ':' {
            return error.InvalidJSON
        }
        item := parse_value(text, skip_space(text, pos + 1), depth)?
        members = append(members, Member{name: name.value, value: item.value})
        pos = skip_space(text, item.pos)
        if pos < len(text) && text[pos] == '}' {
            return Parsed{value: Value.Object(members), pos: pos + 1}
        }
        if pos >= len(text) || text[pos] != ',' {
            return error.InvalidJSON
        }
        pos = skip_space(text, pos + 1)
    }
    return error.InvalidJSON
}

fn parse_string(text str, start i32) !ParsedString {
    sb := sb_new()
    end := parse_string_into(text, start, sb) or |err| {
        sb_discard(sb)
        err?
        return ParsedString{value: "", pos: 0}
    }
    return ParsedString{value: sb_finish(sb), pos: end}
}

fn parse_string_into(text str, start i32, sb i64) !i32 {
    i := start
    for i < len(text) {
        byte := text[i]
        if byte == '"' {
            return i + 1
        }
        if byte < 32 {
            return error.InvalidJSON
        }
        if byte == '\\' {
            i = parse_escape(text, i + 1, sb)?
        } else if byte < 128 {
            sb_write(sb, text[i:i + 1])
            i += 1
        } else {
            width := utf8.width(text, i) or |err| {
                return error.InvalidJSON
            }
            sb_write(sb, text[i:i + width])
            i += width
        }
    }
    return error.InvalidJSON
}

fn parse_escape(text str, pos i32, sb i64) !i32 {
    if pos >= len(text) {
        return error.InvalidJSON
    }
    escape := text[pos]
    if escape == '"' || escape == '\\' || escape == '/' {
        sb_write(sb, text[pos:pos + 1])
    } else if escape == 'b' {
        sb_write(sb, chr(8))
    } else if escape == 'f' {
        sb_write(sb, chr(12))
    } else if escape == 'n' {
        sb_write(sb, "\n")
    } else if escape == 'r' {
        sb_write(sb, "\r")
    } else if escape == 't' {
        sb_write(sb, "\t")
    } else if escape == 'u' {
        return parse_unicode_escape(text, pos + 1, sb)
    } else {
        return error.InvalidJSON
    }
    return pos + 1
}

fn parse_unicode_escape(text str, pos i32, sb i64) !i32 {
    code := hex4(text, pos)
    if code < 0 || (code >= 56320 && code <= 57343) {
        return error.InvalidJSON
    }
    next := pos + 4
    if code >= 55296 && code <= 56319 {
        if next + 6 > len(text) || text[next] != '\\' || text[next + 1] != 'u' {
            return error.InvalidJSON
        }
        low := hex4(text, next + 2)
        if low < 56320 || low > 57343 {
            return error.InvalidJSON
        }
        code = 65536 + (code - 55296) * 1024 + (low - 56320)
        next += 6
    }
    write_code_point(sb, code)
    return next
}

fn hex4(text str, pos i32) i32 {
    if pos + 4 > len(text) {
        return -1
    }
    value := 0
    for i := pos; i < pos + 4; i += 1 {
        digit_value := hex_value(text[i])
        if digit_value < 0 {
            return -1
        }
        value = value * 16 + digit_value
    }
    return value
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

fn write_code_point(sb i64, code i32) void {
    if code < 128 {
        sb_write(sb, chr(code))
    } else if code < 2048 {
        sb_write(sb, chr(192 + code / 64))
        sb_write(sb, chr(128 + code % 64))
    } else if code < 65536 {
        sb_write(sb, chr(224 + code / 4096))
        sb_write(sb, chr(128 + code / 64 % 64))
        sb_write(sb, chr(128 + code % 64))
    } else {
        sb_write(sb, chr(240 + code / 262144))
        sb_write(sb, chr(128 + code / 4096 % 64))
        sb_write(sb, chr(128 + code / 64 % 64))
        sb_write(sb, chr(128 + code % 64))
    }
}

fn encode_into(sb i64, value Value, depth i32) !void {
    match value {
    case Value.Null {
        sb_write(sb, "null")
    }
    case Value.Bool(b) {
        if b.value {
            sb_write(sb, "true")
        } else {
            sb_write(sb, "false")
        }
    }
    case Value.Number(n) {
        if !valid_number(n.text) {
            return error.InvalidNumber
        }
        sb_write(sb, n.text)
    }
    case Value.String(s) {
        encode_string(sb, s.value)?
    }
    case Value.Array(a) {
        if depth >= max_depth() {
            return error.TooDeep
        }
        sb_write(sb, "[")
        for i := 0; i < len(a.items); i += 1 {
            if i > 0 {
                sb_write(sb, ",")
            }
            encode_into(sb, a.items[i], depth + 1)?
        }
        sb_write(sb, "]")
    }
    case Value.Object(o) {
        if depth >= max_depth() {
            return error.TooDeep
        }
        names := map[str]bool{}
        sb_write(sb, "{")
        for i := 0; i < len(o.members); i += 1 {
            member := o.members[i]
            if has(names, member.name) {
                return error.DuplicateName
            }
            names[member.name] = true
            if i > 0 {
                sb_write(sb, ",")
            }
            encode_string(sb, member.name)?
            sb_write(sb, ":")
            encode_into(sb, member.value, depth + 1)?
        }
        sb_write(sb, "}")
    }
    }
}

fn encode_string(sb i64, value str) !void {
    sb_write(sb, "\"")
    i := 0
    for i < len(value) {
        byte := value[i]
        if byte == '"' {
            sb_write(sb, "\\\"")
        } else if byte == '\\' {
            sb_write(sb, "\\\\")
        } else if byte == '\n' {
            sb_write(sb, "\\n")
        } else if byte == '\r' {
            sb_write(sb, "\\r")
        } else if byte == '\t' {
            sb_write(sb, "\\t")
        } else if byte < 32 {
            sb_write(sb, "\\u00")
            sb_write(sb, "0123456789abcdef"[byte / 16:byte / 16 + 1])
            sb_write(sb, "0123456789abcdef"[byte % 16:byte % 16 + 1])
        } else if byte < 128 {
            sb_write(sb, value[i:i + 1])
        } else {
            width := utf8.width(value, i) or |err| {
                return error.InvalidString
            }
            sb_write(sb, value[i:i + width])
            i += width
            continue
        }
        i += 1
    }
    sb_write(sb, "\"")
}

fn valid_number(text str) bool {
    return len(text) > 0 && number_end(text, 0) == len(text)
}

fn integer_text(text str) bool {
    return strings.index(text, ".") < 0 &&
        strings.index(text, "e") < 0 &&
        strings.index(text, "E") < 0
}

fn number_end(text str, start i32) i32 {
    i := start
    if i < len(text) && text[i] == '-' {
        i += 1
    }
    if i >= len(text) || !digit(text[i]) {
        return -1
    }
    if text[i] == '0' {
        i += 1
    } else {
        i = digits_end(text, i)
    }
    if i < len(text) && text[i] == '.' {
        if i + 1 >= len(text) || !digit(text[i + 1]) {
            return -1
        }
        i = digits_end(text, i + 1)
    }
    if i < len(text) && (text[i] == 'e' || text[i] == 'E') {
        i += 1
        if i < len(text) && (text[i] == '+' || text[i] == '-') {
            i += 1
        }
        if i >= len(text) || !digit(text[i]) {
            return -1
        }
        i = digits_end(text, i)
    }
    return i
}

fn digits_end(text str, start i32) i32 {
    i := start
    for i < len(text) && digit(text[i]) {
        i += 1
    }
    return i
}

fn digit(byte i32) bool {
    return byte >= '0' && byte <= '9'
}

fn skip_space(text str, start i32) i32 {
    i := start
    for i < len(text) && (text[i] == ' ' || text[i] == '\t' || text[i] == '\n' || text[i] == '\r') {
        i += 1
    }
    return i
}

fn literal_at(text str, pos i32, word str) bool {
    return pos + len(word) <= len(text) && text[pos:pos + len(word)] == word
}

fn max_depth() i32 {
    return 128
}
