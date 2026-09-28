package main

import "std/conv"
import "std/env"
import "std/http"
import "std/json"
import "std/net"
import "std/stdio"
import "std/strings"

struct Todo {
    id i64
    title str
    done bool
}

struct Reply {
    status i32
    body str
}

enum Command {
    List {
        reply chan[Reply]
    }
    Create {
        title str
        reply chan[Reply]
    }
    Get {
        id i64
        reply chan[Reply]
    }
    Update {
        id i64
        done bool
        reply chan[Reply]
    }
    Delete {
        id i64
        reply chan[Reply]
    }
}

fn main() !i32 {
    port := listen_port()?
    server := http.listen(net.Addr{host: "127.0.0.1", port: port}, http.default_limits())?
    print("listening on http://127.0.0.1:" + to_str(port) + "\n")

    commands := chan_new[Command](64)
    taskgroup []void {
        spawn run_store(commands)
        for i := 0; i < 8; i += 1 {
            spawn serve_connections(server, commands)
        }
    }
    return 0
}

fn listen_port() !i32 {
    value := env.lookup("PORT") or |err| {
        return 8080
    }
    return conv.to_i32(strings.parse_i64(value)?)
}

fn serve_connections(server http.Server, commands chan[Command]) void {
    router := routes(commands) or |err| {
        stdio.eprint("invalid routes: " + to_str(err) + "\n")
        return
    }
    for true {
        connection := server.accept() or |err| {
            stdio.eprint("accept failed: " + to_str(err) + "\n")
            return
        }
        connection.serve(fn(req http.Request) !http.Response {
            return router.serve(req)
        }) or |err| {
            stdio.eprint("request failed: " + to_str(err) + "\n")
        }
    }
}

fn routes(commands chan[Command]) !http.Router {
    return http.router([]http.Route{
        http.route("GET", "/todos", fn(req http.Request) !http.Response {
            reply := chan_new[Reply](1)
            chan_send(commands, Command.List{reply: reply})?
            return respond(chan_recv(reply)?)
        })?,
        http.route("POST", "/todos", fn(req http.Request) !http.Response {
            title := todo_title(req.body) or |err| {
                return error_response(400, "body must be {\"title\": \"...\"}")
            }
            reply := chan_new[Reply](1)
            chan_send(commands, Command.Create{title: title, reply: reply})?
            return respond(chan_recv(reply)?)
        })?,
        http.route("GET", "/todos/{id}", fn(req http.Request) !http.Response {
            id := todo_id(req) or |err| {
                return error_response(400, "invalid id")
            }
            reply := chan_new[Reply](1)
            chan_send(commands, Command.Get{id: id, reply: reply})?
            return respond(chan_recv(reply)?)
        })?,
        http.route("PATCH", "/todos/{id}", fn(req http.Request) !http.Response {
            id := todo_id(req) or |err| {
                return error_response(400, "invalid id")
            }
            done := todo_done(req.body) or |err| {
                return error_response(400, "body must be {\"done\": true|false}")
            }
            reply := chan_new[Reply](1)
            chan_send(commands, Command.Update{id: id, done: done, reply: reply})?
            return respond(chan_recv(reply)?)
        })?,
        http.route("DELETE", "/todos/{id}", fn(req http.Request) !http.Response {
            id := todo_id(req) or |err| {
                return error_response(400, "invalid id")
            }
            reply := chan_new[Reply](1)
            chan_send(commands, Command.Delete{id: id, reply: reply})?
            return respond(chan_recv(reply)?)
        })?,
    })
}

fn todo_id(req http.Request) !i64 {
    return strings.parse_i64(req.path_value("id")?)
}

fn todo_title(body str) !str {
    title := json.as_str(json.get(json.parse(body)?, "title")?)?
    if len(title) == 0 {
        return error.EmptyTitle
    }
    return title
}

fn todo_done(body str) !bool {
    return json.as_bool(json.get(json.parse(body)?, "done")?)
}

error EmptyTitle

fn respond(reply Reply) !http.Response {
    if reply.status == 204 {
        return http.response(204, "")
    }
    return http.json(reply.status, reply.body)
}

fn error_response(status i32, message str) !http.Response {
    body := json.encode(json.Value.Object([]json.Member{
        json.Member{name: "error", value: json.Value.String(message)},
    }))?
    return http.json(status, body)
}

fn run_store(commands chan[Command]) void {
    todos := []Todo{}
    next_id := conv.to_i64(1)
    for true {
        command := chan_recv(commands) or |err| {
            return
        }
        match command {
        case Command.List(c) {
            items := []json.Value{}
            for i := 0; i < len(todos); i += 1 {
                items = append(items, todo_json(todos[i]))
            }
            send_reply(c.reply, 200, json.Value.Array(items))
        }
        case Command.Create(c) {
            todo := Todo{id: next_id, title: c.title, done: false}
            next_id += 1
            todos = append(todos, todo)
            send_reply(c.reply, 201, todo_json(todo))
        }
        case Command.Get(c) {
            index := find_todo(todos, c.id)
            if index < 0 {
                send_not_found(c.reply)
            } else {
                send_reply(c.reply, 200, todo_json(todos[index]))
            }
        }
        case Command.Update(c) {
            index := find_todo(todos, c.id)
            if index < 0 {
                send_not_found(c.reply)
            } else {
                todos[index].done = c.done
                send_reply(c.reply, 200, todo_json(todos[index]))
            }
        }
        case Command.Delete(c) {
            index := find_todo(todos, c.id)
            if index < 0 {
                send_not_found(c.reply)
            } else {
                todos = remove_todo(todos, index)
                chan_send(c.reply, Reply{status: 204, body: ""}) or |err| {
                }
            }
        }
        }
    }
}

fn todo_json(todo Todo) json.Value {
    return json.Value.Object([]json.Member{
        json.Member{name: "id", value: json.int(todo.id)},
        json.Member{name: "title", value: json.Value.String(todo.title)},
        json.Member{name: "done", value: json.Value.Bool(todo.done)},
    })
}

fn send_reply(reply chan[Reply], status i32, value json.Value) void {
    body := json.encode(value) or |err| {
        chan_send(reply, Reply{status: 500, body: "{\"error\":\"encoding failed\"}"}) or |err| {
        }
        return
    }
    chan_send(reply, Reply{status: status, body: body}) or |err| {
    }
}

fn send_not_found(reply chan[Reply]) void {
    chan_send(reply, Reply{status: 404, body: "{\"error\":\"todo not found\"}"}) or |err| {
    }
}

fn find_todo(todos []Todo, id i64) i32 {
    for i := 0; i < len(todos); i += 1 {
        if todos[i].id == id {
            return i
        }
    }
    return -1
}

fn remove_todo(todos []Todo, index i32) []Todo {
    kept := []Todo{}
    for i := 0; i < len(todos); i += 1 {
        if i != index {
            kept = append(kept, todos[i])
        }
    }
    return kept
}
