package http

import "std/net"
import "std/strings"

pub struct Header {
    pub name str
    pub value str
}

pub struct Request {
    pub method str
    pub target str
    pub headers []Header
    pub body str
}

struct HeadRead {
    head str
    rest str
}

struct LineRead {
    line str
    rest str
}

struct ExactRead {
    value str
    rest str
}

struct ParsedHead {
    method str
    target str
    headers []Header
    content_length i32
    chunked bool
    expect_continue bool
}

pub fn (r Request) header(name str) !str {
    if !valid_field_name(name) {
        return error.InvalidArgument
    }
    normalized := strings.to_lower(name)
    for i := 0; i < len(r.headers); i += 1 {
        if r.headers[i].name == normalized {
            return r.headers[i].value
        }
    }
    return error.HeaderNotFound
}

pub fn (r Request) header_values(name str) ![]str {
    values := []str{}
    if !valid_field_name(name) {
        return error.InvalidArgument
    }
    normalized := strings.to_lower(name)
    for i := 0; i < len(r.headers); i += 1 {
        if r.headers[i].name == normalized {
            values = append(values, r.headers[i].value)
        }
    }
    return values
}

fn read_request(conn net.Conn, limits Limits) !Request {
    read := read_head(conn, limits.max_head_bytes)?
    parsed := parse_head(read.head, limits.max_body_bytes)?
    if parsed.expect_continue && (parsed.chunked || parsed.content_length > 0) {
        conn.set_write_deadline_after(limits.write_timeout_millis)?
        write_all(conn, "HTTP/1.1 100 Continue\r\n\r\n")?
    }

    body := ""
    if parsed.chunked {
        body = read_chunked_body(conn, read.rest, limits)?
    } else if parsed.content_length > 0 {
        exact := read_exact(conn, read.rest, parsed.content_length)?
        body = exact.value
    }
    return Request{
        method: parsed.method,
        target: parsed.target,
        headers: parsed.headers,
        body: body,
    }
}

fn read_head(conn net.Conn, max_bytes i32) !HeadRead {
    parts := []str{}
    total := 0
    tail := ""
    for true {
        if total >= max_bytes {
            reject_head_limit(join_parts(parts))?
            return error.BadRequest
        }

        remaining := max_bytes - total
        read_size := 4096
        if remaining < read_size {
            read_size = remaining
        }
        chunk := conn.read(read_size)?
        if len(chunk) == 0 {
            return error.BadRequest
        }
        combined := tail + chunk
        relative_end := strings.index(combined, "\r\n\r\n")
        parts = append(parts, chunk)
        if relative_end >= 0 {
            head_end := total - len(tail) + relative_end + 4
            buffered := join_parts(parts)
            return HeadRead{head: buffered[0:head_end], rest: buffered[head_end:]}
        }
        total += len(chunk)
        tail = suffix(combined, 3)
    }
    return error.BadRequest
}

fn head_limit_error(buffered str) error {
    if strings.index(buffered, "\r\n") >= 0 {
        return error.HeaderTooLarge
    }
    first_space := strings.index(buffered, " ")
    if first_space < 0 {
        return error.BadRequest
    }
    if strings.index(buffered[first_space + 1:], " ") < 0 {
        return error.URITooLong
    }
    return error.BadRequest
}

fn reject_head_limit(buffered str) !void {
    err := head_limit_error(buffered)
    err?
    return
}

fn parse_head(head str, max_body_bytes i32) !ParsedHead {
    lines := strings.split(head[0:len(head) - 4], "\r\n")
    if len(lines) == 0 {
        return error.BadRequest
    }
    request_line := strings.split(lines[0], " ")
    if len(request_line) != 3 || len(request_line[0]) == 0 || len(request_line[1]) == 0 {
        return error.BadRequest
    }
    if !valid_token(request_line[0]) {
        return error.BadRequest
    }
    if !valid_http_version(request_line[2]) {
        return error.BadRequest
    }
    if request_line[2] != "HTTP/1.1" {
        return error.HTTPVersionNotSupported
    }
    if request_line[0] == "CONNECT" {
        return error.UnsupportedMethod
    }
    effective_authority := parse_target_authority(request_line[0], request_line[1])?

    headers := []Header{}
    host_count := 0
    content_length_count := 0
    transfer_encoding_count := 0
    expect_count := 0
    content_length_value := ""
    transfer_encoding_value := ""
    expect_value := ""

    for i := 1; i < len(lines); i += 1 {
        header := parse_header_line(lines[i])?
        headers = append(headers, header)
        if header.name == "host" {
            host_count += 1
            if !valid_authority(header.value) {
                return error.BadRequest
            }
        } else if header.name == "content-length" {
            content_length_count += 1
            content_length_value = header.value
        } else if header.name == "transfer-encoding" {
            transfer_encoding_count += 1
            transfer_encoding_value = header.value
        } else if header.name == "expect" {
            expect_count += 1
            expect_value = header.value
        }
    }

    if host_count != 1 || content_length_count > 1 {
        return error.BadRequest
    }
    if len(effective_authority) != 0 {
        for i := 0; i < len(headers); i += 1 {
            if headers[i].name == "host" {
                headers[i].value = effective_authority
            }
        }
    }
    if content_length_count > 0 && transfer_encoding_count > 0 {
        return error.BadRequest
    }
    if transfer_encoding_count > 1 {
        return error.UnsupportedTransferEncoding
    }
    chunked := false
    if transfer_encoding_count == 1 {
        if strings.to_lower(strings.trim(transfer_encoding_value, " \t")) != "chunked" {
            return error.UnsupportedTransferEncoding
        }
        chunked = true
    }

    content_length := 0
    if content_length_count == 1 {
        content_length = parse_decimal_length(
            strings.trim(content_length_value, " \t"),
            max_body_bytes,
        )?
    }

    expect_continue := false
    if expect_count > 1 {
        return error.ExpectationFailed
    }
    if expect_count == 1 {
        if strings.to_lower(strings.trim(expect_value, " \t")) != "100-continue" {
            return error.ExpectationFailed
        }
        expect_continue = true
    }

    return ParsedHead{
        method: request_line[0],
        target: request_line[1],
        headers: headers,
        content_length: content_length,
        chunked: chunked,
        expect_continue: expect_continue,
    }
}

fn parse_header_line(line str) !Header {
    colon := strings.index(line, ":")
    if colon <= 0 {
        return error.BadRequest
    }
    name := line[0:colon]
    value := strings.trim(line[colon + 1:], " \t")
    if !valid_field_name(name) || !valid_field_value(value) {
        return error.BadRequest
    }
    return Header{name: strings.to_lower(name), value: value}
}

fn parse_decimal_length(value str, max_value i32) !i32 {
    if len(value) == 0 {
        return error.BadRequest
    }
    result := 0
    for i := 0; i < len(value); i += 1 {
        digit := value[i] - '0'
        if digit < 0 || digit > 9 {
            return error.BadRequest
        }
        if digit > max_value || result > (max_value - digit) / 10 {
            return error.BodyTooLarge
        }
        result = result * 10 + digit
    }
    return result
}

fn read_chunked_body(conn net.Conn, buffered str, limits Limits) !str {
    builder := sb_new()
    read_chunked_into(conn, buffered, limits, builder) or |err| {
        sb_discard(builder)
        err?
        return ""
    }
    return sb_finish(builder)
}

fn read_chunked_into(conn net.Conn, buffered str, limits Limits, builder i64) !void {
    rest := buffered
    total := 0
    metadata_bytes := 0

    for true {
        line_read := read_line(conn, rest, limits.max_head_bytes - metadata_bytes)?
        rest = line_read.rest
        metadata_bytes += len(line_read.line) + 2
        if metadata_bytes > limits.max_head_bytes {
            return error.HeaderTooLarge
        }

        size_text := line_read.line
        semicolon := strings.index(size_text, ";")
        if semicolon >= 0 {
            if !valid_chunk_extensions(size_text[semicolon + 1:]) {
                return error.BadRequest
            }
            size_text = strings.trim(size_text[0:semicolon], " \t")
        }
        chunk_size := parse_hex_length(size_text, limits.max_body_bytes - total)?
        if chunk_size == 0 {
            read_trailers(conn, rest, limits.max_head_bytes - metadata_bytes)?
            return
        }

        exact := read_exact(conn, rest, chunk_size + 2)?
        if exact.value[chunk_size:] != "\r\n" {
            return error.BadRequest
        }
        sb_write(builder, exact.value[0:chunk_size])
        total += chunk_size
        rest = exact.rest
    }
    return
}

fn read_trailers(conn net.Conn, buffered str, remaining_metadata i32) !void {
    rest := buffered
    remaining := remaining_metadata
    for true {
        line_read := read_line(conn, rest, remaining)?
        used := len(line_read.line) + 2
        if used > remaining {
            return error.HeaderTooLarge
        }
        remaining -= used
        rest = line_read.rest
        if len(line_read.line) == 0 {
            return
        }
        trailer := parse_header_line(line_read.line)?
        if forbidden_trailer_field(trailer.name) {
            return error.BadRequest
        }
    }
    return error.BadRequest
}

fn read_line(conn net.Conn, buffered str, max_bytes i32) !LineRead {
    if max_bytes < 2 {
        return error.HeaderTooLarge
    }
    end := strings.index(buffered, "\r\n")
    if end >= 0 {
        if end + 2 > max_bytes {
            return error.HeaderTooLarge
        }
        return LineRead{line: buffered[0:end], rest: buffered[end + 2:]}
    }
    if len(buffered) >= max_bytes {
        return error.HeaderTooLarge
    }

    parts := []str{buffered}
    total := len(buffered)
    tail := suffix(buffered, 1)
    for true {
        if total >= max_bytes {
            return error.HeaderTooLarge
        }
        read_size := 4096
        remaining := max_bytes - total
        if remaining < read_size {
            read_size = remaining
        }
        chunk := conn.read(read_size)?
        if len(chunk) == 0 {
            return error.BadRequest
        }
        combined := tail + chunk
        relative_end := strings.index(combined, "\r\n")
        parts = append(parts, chunk)
        if relative_end >= 0 {
            end = total - len(tail) + relative_end
            rest := join_parts(parts)
            return LineRead{line: rest[0:end], rest: rest[end + 2:]}
        }
        total += len(chunk)
        tail = suffix(combined, 1)
    }
    return error.BadRequest
}

fn join_parts(parts []str) str {
    builder := sb_new()
    for i := 0; i < len(parts); i += 1 {
        sb_write(builder, parts[i])
    }
    return sb_finish(builder)
}

fn suffix(value str, max_bytes i32) str {
    if len(value) <= max_bytes {
        return value
    }
    return value[len(value) - max_bytes:]
}

fn read_exact(conn net.Conn, buffered str, size i32) !ExactRead {
    if len(buffered) >= size {
        return ExactRead{value: buffered[0:size], rest: buffered[size:]}
    }

    builder := sb_new()
    sb_write(builder, buffered)
    read_exact_into(conn, builder, size - len(buffered)) or |err| {
        sb_discard(builder)
        err?
        return ExactRead{value: "", rest: ""}
    }
    return ExactRead{value: sb_finish(builder), rest: ""}
}

fn read_exact_into(conn net.Conn, builder i64, remaining i32) !void {
    unread := remaining
    for unread > 0 {
        read_size := unread
        if read_size > 65536 {
            read_size = 65536
        }
        chunk := conn.read(read_size)?
        if len(chunk) == 0 {
            return error.BadRequest
        }
        sb_write(builder, chunk)
        unread -= len(chunk)
    }
    return
}

fn parse_hex_length(value str, max_value i32) !i32 {
    if len(value) == 0 {
        return error.BadRequest
    }
    result := 0
    for i := 0; i < len(value); i += 1 {
        digit := hex_digit(value[i])
        if digit < 0 {
            return error.BadRequest
        }
        if digit > max_value || result > (max_value - digit) / 16 {
            return error.BodyTooLarge
        }
        result = result * 16 + digit
    }
    return result
}

fn hex_digit(value i32) i32 {
    if value >= '0' && value <= '9' {
        return value - '0'
    }
    if value >= 'a' && value <= 'f' {
        return value - 'a' + 10
    }
    if value >= 'A' && value <= 'F' {
        return value - 'A' + 10
    }
    return -1
}

fn forbidden_trailer_field(name str) bool {
    return name == "connection" ||
        name == "content-length" ||
        name == "host" ||
        name == "trailer" ||
        name == "transfer-encoding" ||
        name == "upgrade"
}

fn valid_field_name(value str) bool {
    return valid_token(value)
}

fn valid_token(value str) bool {
    if len(value) == 0 {
        return false
    }
    for i := 0; i < len(value); i += 1 {
        if !token_byte(value[i]) {
            return false
        }
    }
    return true
}

fn token_byte(value i32) bool {
    if value >= '0' && value <= '9' {
        return true
    }
    if value >= 'A' && value <= 'Z' {
        return true
    }
    if value >= 'a' && value <= 'z' {
        return true
    }
    return value == '!' || value == '#' || value == '$' || value == '%' ||
        value == '&' || value == '\'' || value == '*' || value == '+' ||
        value == '-' || value == '.' || value == '^' || value == '_' ||
        value == '`' || value == '|' || value == '~'
}

fn valid_field_value(value str) bool {
    for i := 0; i < len(value); i += 1 {
        byte := value[i]
        if (byte < 32 && byte != '\t') || byte == 127 {
            return false
        }
    }
    return true
}
