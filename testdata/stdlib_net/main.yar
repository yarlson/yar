package main

import "net"

fn main() !i32 {
    ln := net.listen("127.0.0.1", 0)?
    addr := net.listener_addr(ln)?

    client := net.connect("127.0.0.1", addr.port)?
    server := net.accept(ln)?

    net.write(client, "hello")?

    data := net.read(server, 4096)?
    if data != "hello" {
        print("read mismatch\n")
        return 1
    }

    net.write(server, "world")?
    reply := net.read(client, 4096)?
    if reply != "world" {
        print("reply mismatch\n")
        return 1
    }

    ra := net.remote_addr(server)?
    la := net.local_addr(client)?
    if ra.port != la.port {
        print("addr mismatch\n")
        return 1
    }

    resolved := net.resolve("127.0.0.1", 80)?
    if resolved.host != "127.0.0.1" {
        print("resolve mismatch\n")
        return 1
    }

    net.close(client)?
    net.close(server)?
    net.close_listener(ln)?

    print("net ok\n")
    return 0
}
