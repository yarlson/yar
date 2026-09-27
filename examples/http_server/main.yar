package main

import "std/http"
import "std/net"

fn handle(req http.Request) !http.Response {
    if req.method == "GET" && req.target == "/health" {
        return http.text(200, "ok\n")
    }
    return http.text(404, "not found\n")
}

fn main() !i32 {
    server := http.listen(
        net.Addr{host: "127.0.0.1", port: 8080},
        http.default_limits(),
    )?
    print("listening on http://127.0.0.1:8080\n")

    for true {
        connection := server.accept()?
        connection.serve(fn(req http.Request) !http.Response {
            return handle(req)
        }) or |err| {
            print("http connection failed: " + to_str(err) + "\n")
        }
    }
    return 0
}
