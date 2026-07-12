package main

import "std/net"

fn write_all(conn net.Conn, data str) !void {
    off := 0
    for off < len(data) {
        written := conn.write(data[off:])?
        if written <= 0 {
            return error.IO
        }
        off += written
    }
    return
}

fn main() !i32 {
    listener := net.listen_stream("127.0.0.1", 0)?
    addr := listener.addr()?

    results := taskgroup []!i32 {
        spawn serve(listener)
        spawn exchange(addr.port)
    }
    if results[0]? != 0 || results[1]? != 0 {
        listener.close()?
        return 1
    }

    resolved := net.resolve("127.0.0.1", 80)?
    if resolved.host != "127.0.0.1" {
        listener.close()?
        return 1
    }

    listener.close()?
    print("net ok\n")
    return 0
}

fn serve(listener net.Listener) !i32 {
    conn := listener.accept()?
    data := conn.read(4096)?
    if data != "hello" {
        conn.close()?
        return 1
    }
    write_all(conn, "world")?
    conn.close()?
    return 0
}

fn exchange(port i32) !i32 {
    conn := net.connect_stream("127.0.0.1", port)?
    write_all(conn, "hello")?
    reply := conn.read(4096)?
    if reply != "world" {
        conn.close()?
        return 1
    }

    remote := conn.remote_addr()?
    local := conn.local_addr()?
    if remote.port != port || local.port <= 0 {
        conn.close()?
        return 1
    }

    conn.close()?
    return 0
}
