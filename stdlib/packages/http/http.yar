package http

import "std/conv"
import "std/net"
import "std/sort"
import "std/strings"

pub struct Request {
    method str
    path str
    headers map[str]str
    body str
}

pub struct Response {
    status i32
    headers map[str]str
    body str
}

pub fn text(status i32, body str) Response {
    headers := map[str]str{}
    headers["content-type"] = "text/plain; charset=utf-8"
    return Response{status: status, headers: headers, body: body}
}

pub fn serve(addr net.Addr, handler fn(Request) !Response) !void {
    listener := net.listen(addr.host, addr.port)?

    for true {
        conn := net.accept(listener) or |err| {
            break
        }
        handle(conn, handler)
    }

    close_listener_quiet(listener)
    return error.IO
}

fn handle(conn i64, handler fn(Request) !Response) void {
    req := read_request(conn) or |err| {
        write_response_quiet(conn, text(400, "bad request\n"))
        close_quiet(conn)
        return
    }

    resp := handler(req) or |err| {
        write_response_quiet(conn, text(500, "internal server error\n"))
        close_quiet(conn)
        return
    }

    write_response_quiet(conn, resp)
    close_quiet(conn)
}

fn write_response_quiet(conn i64, resp Response) void {
    write_response(conn, resp) or |err| {
    }
}

fn close_quiet(conn i64) void {
    net.close(conn) or |err| {
    }
}

fn close_listener_quiet(listener i64) void {
    net.close_listener(listener) or |err| {
    }
}

fn read_request(conn i64) !Request {
    raw := net.read(conn, 65536)?
    if len(raw) == 0 {
        return error.InvalidRequest
    }

    header_end := strings.index(raw, "\r\n\r\n")
    separator_len := 4
    if header_end < 0 {
        header_end = strings.index(raw, "\n\n")
        separator_len = 2
    }
    if header_end < 0 {
        return error.InvalidRequest
    }

    head := strings.replace(raw[0:header_end], "\r\n", "\n", -1)
    body := raw[header_end + separator_len:len(raw)]
    lines := strings.split(head, "\n")
    if len(lines) == 0 {
        return error.InvalidRequest
    }

    parts := strings.split(lines[0], " ")
    if len(parts) < 2 {
        return error.InvalidRequest
    }

    headers := parse_headers(lines)?
    if has(headers, "content-length") {
        want64 := strings.parse_i64(headers["content-length"]?)?
        if want64 < conv.to_i64(0) || want64 > conv.to_i64(65536) {
            return error.InvalidRequest
        }
        want := conv.to_i32(want64)
        for len(body) < want {
            chunk := net.read(conn, want - len(body))?
            if len(chunk) == 0 {
                return error.InvalidRequest
            }
            body = body + chunk
        }
        if len(body) > want {
            body = body[0:want]
        }
    }

    return Request{method: parts[0], path: parts[1], headers: headers, body: body}
}

fn parse_headers(lines []str) !map[str]str {
    headers := map[str]str{}
    i := 1
    for i < len(lines) {
        line := lines[i]
        if len(line) > 0 {
            colon := strings.index(line, ":")
            if colon <= 0 {
                return error.InvalidRequest
            }
            name := strings.to_lower(strings.trim(line[0:colon], " \t"))
            value := strings.trim(line[colon + 1:len(line)], " \t")
            if len(name) == 0 {
                return error.InvalidRequest
            }
            headers[name] = value
        }
        i = i + 1
    }
    return headers
}

fn write_response(conn i64, resp Response) !void {
    headers := resp.headers
    headers["content-length"] = to_str(len(resp.body))
    if !has(headers, "content-type") {
        headers["content-type"] = "text/plain; charset=utf-8"
    }

    data := "HTTP/1.1 " + to_str(resp.status) + " " + status_text(resp.status) + "\r\n"
    names := keys(headers)
    sort.strings(names)

    i := 0
    for i < len(names) {
        name := names[i]
        value := headers[name]?
        data = data + name + ": " + value + "\r\n"
        i = i + 1
    }

    data = data + "\r\n" + resp.body
    net.write(conn, data)?
    return
}

fn status_text(status i32) str {
    if status == 200 {
        return "OK"
    }
    if status == 201 {
        return "Created"
    }
    if status == 204 {
        return "No Content"
    }
    if status == 400 {
        return "Bad Request"
    }
    if status == 405 {
        return "Method Not Allowed"
    }
    if status == 404 {
        return "Not Found"
    }
    if status == 500 {
        return "Internal Server Error"
    }
    return "Status"
}
