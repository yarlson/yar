package main

import "std/http"

fn handle(req http.Request) !http.Response {
    if req.method != "POST" {
        return http.text(405, "method\n")
    }
    if req.path != "/echo" {
        return http.text(404, "missing\n")
    }
    content_type := req.headers["content-type"]?
    return http.text(200, content_type + ":" + req.body)
}

fn main() i32 {
    resp := handle(http.Request{
        method: "POST",
        path: "/echo",
        headers: map[str]str{"content-type": "text/plain"},
        body: "hello",
    }) or |err| {
        print("handler failed\n")
        return 1
    }

    if resp.status != 200 {
        print("status mismatch\n")
        return 1
    }
    if resp.body != "text/plain:hello" {
        print("body mismatch\n")
        return 1
    }

    print("http ok\n")
    return 0
}
