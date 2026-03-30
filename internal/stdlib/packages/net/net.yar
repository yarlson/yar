package net

pub struct Addr {
    host str
    port i32
}

pub fn listen(host str, port i32) !i64 {
    panic("net.listen intrinsic")
}

pub fn accept(listener i64) !i64 {
    panic("net.accept intrinsic")
}

pub fn listener_addr(listener i64) !Addr {
    panic("net.listener_addr intrinsic")
}

pub fn close_listener(listener i64) !void {
    panic("net.close_listener intrinsic")
}

pub fn connect(host str, port i32) !i64 {
    panic("net.connect intrinsic")
}

pub fn read(conn i64, max_bytes i32) !str {
    panic("net.read intrinsic")
}

pub fn write(conn i64, data str) !i32 {
    panic("net.write intrinsic")
}

pub fn close(conn i64) !void {
    panic("net.close intrinsic")
}

pub fn local_addr(conn i64) !Addr {
    panic("net.local_addr intrinsic")
}

pub fn remote_addr(conn i64) !Addr {
    panic("net.remote_addr intrinsic")
}

pub fn set_read_deadline(conn i64, millis i32) !void {
    panic("net.set_read_deadline intrinsic")
}

pub fn set_write_deadline(conn i64, millis i32) !void {
    panic("net.set_write_deadline intrinsic")
}

pub fn resolve(host str, port i32) !Addr {
    panic("net.resolve intrinsic")
}
