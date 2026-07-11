package main

import "std/http"
import "std/net"

fn handle(req http.Request) !http.Response {
    if req.path == "/health" {
        return http.text(200, "ok\n")
    }
    return http.text(200, "hello from Yar\n")
}

fn main() !i32 {
    print("listening on http://127.0.0.1:8080\n")
    http.serve(net.Addr{host: "127.0.0.1", port: 8080}, fn(req http.Request) !http.Response {
        return handle(req)
    })?
    return 0
}
