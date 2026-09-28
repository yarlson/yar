package http

import "std/net"
import "std/strings"

pub struct Response {
    status i32
    headers []Header
    body str
}

pub fn response(status i32, body str) !Response {
    if status < 200 || status > 599 {
        return error.InvalidResponse
    }
    if status_forbids_body(status) && len(body) != 0 {
        return error.InvalidResponse
    }
    return Response{status: status, headers: []Header{}, body: body}
}

pub fn text(status i32, body str) !Response {
    out := response(status, body)?
    return out.with_header("content-type", "text/plain; charset=utf-8")
}

pub fn json(status i32, body str) !Response {
    out := response(status, body)?
    return out.with_header("content-type", "application/json")
}

pub fn (r Response) status() i32 {
    return r.status
}

pub fn (r Response) body() str {
    return r.body
}

pub fn (r Response) header(name str) !str {
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

pub fn (r Response) with_header(name str, value str) !Response {
    header := response_header(name, value)?
    headers := []Header{}
    replaced := false
    for i := 0; i < len(r.headers); i += 1 {
        if r.headers[i].name == header.name {
            if !replaced {
                headers = append(headers, header)
                replaced = true
            }
        } else {
            headers = append(headers, r.headers[i])
        }
    }
    if !replaced {
        headers = append(headers, header)
    }
    r.headers = headers
    return r
}

pub fn (r Response) add_header(name str, value str) !Response {
    header := response_header(name, value)?
    headers := []Header{}
    for i := 0; i < len(r.headers); i += 1 {
        headers = append(headers, r.headers[i])
    }
    headers = append(headers, header)
    r.headers = headers
    return r
}

fn write_response(conn net.Conn, method str, resp Response, max_head_bytes i32) !void {
    if resp.status < 200 || resp.status > 599 {
        return error.InvalidResponse
    }
    if status_forbids_body(resp.status) && len(resp.body) != 0 {
        return error.InvalidResponse
    }
    for i := 0; i < len(resp.headers); i += 1 {
        if !valid_field_name(resp.headers[i].name) || !valid_field_value(resp.headers[i].value) {
            return error.InvalidResponse
        }
        if reserved_response_field(resp.headers[i].name) {
            return error.InvalidResponse
        }
    }
    if !response_head_fits(resp, max_head_bytes) {
        return error.InvalidResponse
    }

    builder := sb_new()
    sb_write(builder, "HTTP/1.1 " + to_str(resp.status) + " \r\n")
    if resp.status != 204 && resp.status != 304 {
        sb_write(builder, "content-length: " + to_str(len(resp.body)) + "\r\n")
    }
    sb_write(builder, "connection: close\r\n")
    for i := 0; i < len(resp.headers); i += 1 {
        sb_write(builder, resp.headers[i].name + ": " + resp.headers[i].value + "\r\n")
    }
    sb_write(builder, "\r\n")
    write_all(conn, sb_finish(builder))?
    if method != "HEAD" && !status_forbids_body(resp.status) {
        write_all(conn, resp.body)?
    }
    return
}

fn response_head_fits(resp Response, max_bytes i32) bool {
    remaining := max_bytes - 64
    if remaining < 0 {
        return false
    }
    for i := 0; i < len(resp.headers); i += 1 {
        field_bytes := len(resp.headers[i].name) + len(resp.headers[i].value) + 4
        if field_bytes > remaining {
            return false
        }
        remaining -= field_bytes
    }
    return true
}

fn write_error_response(conn net.Conn, status i32) !void {
    data := "HTTP/1.1 " + to_str(status) + " \r\n" +
        "content-length: 0\r\n" +
        "connection: close\r\n\r\n"
    write_all(conn, data)?
    return
}

fn write_all(conn net.Conn, data str) !void {
    offset := 0
    for offset < len(data) {
        written := conn.write(data[offset:])?
        if written <= 0 {
            return net.IO
        }
        offset += written
    }
    return
}

fn status_forbids_body(status i32) bool {
    return status == 204 || status == 205 || status == 304
}

fn reserved_response_field(name str) bool {
    return name == "connection" ||
        name == "content-length" ||
        name == "trailer" ||
        name == "transfer-encoding" ||
        name == "upgrade"
}

fn response_header(name str, value str) !Header {
    if !valid_field_name(name) || !valid_field_value(value) {
        return error.InvalidResponse
    }
    normalized := strings.to_lower(name)
    if reserved_response_field(normalized) {
        return error.InvalidResponse
    }
    return Header{name: normalized, value: value}
}
