package main

import "std/http"
import "std/net"
import "std/strings"

error IO
error HandlerFailed

fn main() !i32 {
    expect_invalid_configuration()?
    expect_invalid_response_header()?
    expect_invalid_request_header_name()?

    server := http.listen(
        net.Addr{host: "127.0.0.1", port: 0},
        http.limits(4096, 1024, 2000, 2000)?,
    )?
    addr := server.addr()?

    results := taskgroup []!i32 {
        spawn serve_requests(server)
        spawn run_clients(addr.port)
    }
    if results[0]? != 0 || results[1]? != 0 {
        server.close()?
        return 1
    }

    server.close()?
    print("http ok\n")
    return 0
}

fn serve_requests(server http.Server) !i32 {
    first := server.accept()?
    if first.local_addr()?.port <= 0 || first.remote_addr()?.port <= 0 {
        return 1
    }
    first.serve(fn(req http.Request) !http.Response {
        return handle(req)
    })?

    second := server.accept()?
    second.serve(fn(req http.Request) !http.Response {
        return handle(req)
    })?

    invalid := server.accept()?
    expect_bad_request(invalid)?

    failed := server.accept()?
    expect_handler_failure(failed)?

    head := server.accept()?
    head.serve(fn(req http.Request) !http.Response {
        return handle(req)
    })?
    return 0
}

fn expect_handler_failure(conn http.Connection) !void {
    conn.serve(fn(req http.Request) !http.Response {
        return error.HandlerFailed
    }) or |err| {
        if err == error.HandlerFailed {
            return
        }
        err?
        return
    }
    return error.IO
}

fn handle(req http.Request) !http.Response {
    host := req.header("Host")?
    if host != "localhost" {
        return error.IO
    }
    if req.target == "/length" {
        if req.method != "POST" || req.body != "hello" {
            return error.IO
        }
        response := http.text(201, req.body)?
        return response.with_header("x-yar", "length")
    }
    if req.target == "/chunked" {
        values := req.header_values("x-part")?
        if req.body != "Wikipedia" || len(values) != 2 {
            return error.IO
        }
        return http.text(200, req.body)
    }
    if req.target == "/head" {
        return http.text(200, "hidden")
    }
    return http.text(404, "not found")
}

fn expect_bad_request(conn http.Connection) !void {
    conn.serve(fn(req http.Request) !http.Response {
        return handle(req)
    }) or |err| {
        if err == http.BadRequest {
            return
        }
        err?
        return
    }
    return error.IO
}

fn run_clients(port i32) !i32 {
    first := net.connect_stream("127.0.0.1", port)?
    write_all(first, "POST /length HTTP/1.1\r\nHost: local")?
    write_all(first, "host\r\nContent-Length: 5\r\n\r\nhe")?
    write_all(first, "llo")?
    first_response := read_all(first)?
    first.close()?
    if !strings.has_prefix(first_response, "HTTP/1.1 201 \r\n") ||
        !strings.contains(first_response, "x-yar: length\r\n") ||
        !strings.has_suffix(first_response, "\r\n\r\nhello") {
        return 1
    }

    second := net.connect_stream("127.0.0.1", port)?
    write_all(second, "POST /chunked HTTP/1.1\r\nHost: localhost\r\n")?
    write_all(second, "X-Part: one\r\nX-Part: two\r\nTransfer-Encoding: chunked\r\n\r\n")?
    write_all(second, "4\r\nWiki\r\n5\r\npedia\r\n0\r\nX-Trailer: done\r\n\r\n")?
    second_response := read_all(second)?
    second.close()?
    if !strings.has_prefix(second_response, "HTTP/1.1 200 \r\n") ||
        !strings.has_suffix(second_response, "\r\n\r\nWikipedia") {
        return 1
    }

    invalid := net.connect_stream("127.0.0.1", port)?
    write_all(
        invalid,
        "POST /bad HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n",
    )?
    invalid_response := read_all(invalid)?
    invalid.close()?
    if !strings.has_prefix(invalid_response, "HTTP/1.1 400 \r\n") {
        return 1
    }

    failed := net.connect_stream("127.0.0.1", port)?
    write_all(failed, "GET /failure HTTP/1.1\r\nHost: localhost\r\n\r\n")?
    failed_response := read_all(failed)?
    failed.close()?
    if !strings.has_prefix(failed_response, "HTTP/1.1 500 \r\n") ||
        !strings.contains(failed_response, "content-length: 0\r\n") ||
        !strings.has_suffix(failed_response, "\r\n\r\n") {
        return 1
    }

    head := net.connect_stream("127.0.0.1", port)?
    write_all(head, "HEAD /head HTTP/1.1\r\nHost: localhost\r\n\r\n")?
    head_response := read_all(head)?
    head.close()?
    if !strings.contains(head_response, "content-length: 6\r\n") ||
        !strings.has_suffix(head_response, "\r\n\r\n") {
        return 1
    }
    return 0
}

fn expect_invalid_configuration() !void {
    value := http.limits(1, 1, 1, 1) or |err| {
        if err == http.InvalidArgument {
            return
        }
        err?
        return
    }
    return error.IO
}

fn expect_invalid_response_header() !void {
    response := http.text(200, "ok")?
    invalid := response.with_header("x-test", "safe\r\ninjected: yes") or |err| {
        if err == http.InvalidResponse {
            return
        }
        err?
        return
    }
    return error.IO
}

fn expect_invalid_request_header_name() !void {
    request := http.Request{method: "GET", target: "/", headers: []http.Header{}, body: ""}
    values := request.header_values("bad name") or |err| {
        if err == http.InvalidArgument {
            return
        }
        err?
        return
    }
    return error.IO
}

fn write_all(conn net.Conn, data str) !void {
    offset := 0
    for offset < len(data) {
        written := conn.write(data[offset:])?
        if written <= 0 {
            return error.IO
        }
        offset += written
    }
    return
}

fn read_all(conn net.Conn) !str {
    builder := sb_new()
    for true {
        chunk := conn.read(4096) or |err| {
            sb_discard(builder)
            err?
            return ""
        }
        if len(chunk) == 0 {
            return sb_finish(builder)
        }
        sb_write(builder, chunk)
    }
    sb_discard(builder)
    return ""
}
