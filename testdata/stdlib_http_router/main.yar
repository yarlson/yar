package main

import "std/http"

fn main() !i32 {
    router := http.router([]http.Route{
        http.route("GET", "/", fn(req http.Request) !http.Response {
            return http.text(200, "root")
        })?,
        http.route("GET", "/users", fn(req http.Request) !http.Response {
            query := req.query()?
            limit := query.get("limit") or |err| {
                return http.text(200, "list")
            }
            return http.text(200, "list:" + limit)
        })?,
        http.route("POST", "/users", fn(req http.Request) !http.Response {
            return http.json(201, req.body)
        })?,
        http.route("GET", "/users/{id}", fn(req http.Request) !http.Response {
            return http.text(200, "user:" + req.path_value("id")?)
        })?,
        http.route("GET", "/users/new", fn(req http.Request) !http.Response {
            return http.text(200, "new")
        })?,
        http.route("DELETE", "/users/{id}", fn(req http.Request) !http.Response {
            return http.response(204, "")
        })?,
        http.route("GET", "/users/{id}/posts/{post}", fn(req http.Request) !http.Response {
            return http.text(200, req.path_value("id")? + "/" + req.path_value("post")?)
        })?,
        http.route("GET", "/files/{path...}", fn(req http.Request) !http.Response {
            return http.text(200, "file:" + req.path_value("path")?)
        })?,
    })?

    expect(router, "GET", "/", 200, "root")?
    expect(router, "GET", "/users", 200, "list")?
    expect(router, "GET", "/users?limit=10", 200, "list:10")?
    expect(router, "GET", "/users/42", 200, "user:42")?
    expect(router, "GET", "/users/new", 200, "new")?
    expect(router, "GET", "/users/a%20b", 200, "user:a b")?
    expect(router, "GET", "/users/a%2Fb", 200, "user:a/b")?
    expect(router, "GET", "/users/7/posts/9", 200, "7/9")?
    expect(router, "GET", "/files/a/b%20c", 200, "file:a/b c")?
    expect(router, "GET", "/files/", 200, "file:")?
    expect(router, "GET", "http://example.com/users/5?x=1", 200, "user:5")?
    expect(router, "HEAD", "/users/42", 200, "user:42")?
    expect(router, "DELETE", "/users/42", 204, "")?
    expect(router, "GET", "/files", 404, "")?
    expect(router, "GET", "/users/", 404, "")?
    expect(router, "GET", "/missing", 404, "")?
    expect(router, "GET", "/users/%zz", 400, "")?

    created := router.serve(request("POST", "/users", "{}"))?
    if created.status() != 201 || created.body() != "{}" {
        return 1
    }
    if created.header("Content-Type")? != "application/json" {
        return 2
    }

    not_allowed := router.serve(request("PUT", "/users/42", ""))?
    if not_allowed.status() != 405 {
        return 3
    }
    if not_allowed.header("allow")? != "GET, HEAD, DELETE" {
        return 4
    }

    if !rejected_route("GET", "users") ||
        !rejected_route("GET", "/a//b") ||
        !rejected_route("GET", "/{rest...}/tail") ||
        !rejected_route("GET", "/{id}/{id}") ||
        !rejected_route("GET", "/{1id}") ||
        !rejected_route("GET", "/a%20b") ||
        !rejected_route("BAD METHOD", "/") {
        return 5
    }
    if !conflicting_routes("/users/{id}", "/users/{name}") {
        return 6
    }
    if conflicting_routes("/users/{id}", "/users/new") {
        return 7
    }

    missing := request("GET", "/", "")
    value := missing.path_value("id") or |err| {
        if err != http.PathValueNotFound {
            return 8
        }
        print("router ok\n")
        return 0
    }
    _ := value
    return 9
}

error Mismatch

fn request(method str, target str, body str) http.Request {
    return http.Request{method: method, target: target, headers: []http.Header{}, body: body}
}

fn expect(router http.Router, method str, target str, status i32, body str) !void {
    resp := router.serve(request(method, target, ""))?
    if resp.status() != status || resp.body() != body {
        print(method + " " + target + " -> " + to_str(resp.status()) + " " + resp.body() + "\n")
        return error.Mismatch
    }
}

fn handler(req http.Request) !http.Response {
    return http.text(200, "")
}

fn rejected_route(method str, pattern str) bool {
    route := http.route(method, pattern, handler) or |err| {
        return err == http.InvalidArgument
    }
    _ := route
    return false
}

fn conflicting_routes(first str, second str) bool {
    router := http.router([]http.Route{
        http.route("GET", first, handler) or |err| {
            return false
        },
        http.route("GET", second, handler) or |err| {
            return false
        },
    }) or |err| {
        return err == http.RouteConflict
    }
    _ := router
    return false
}
