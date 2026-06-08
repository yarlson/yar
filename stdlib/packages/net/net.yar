package net

pub struct Addr {
    host str
    port i32
}

pub struct Conn {
    handle i64
}

pub struct Listener {
    handle i64
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

pub fn listen_stream(host str, port i32) !Listener {
    handle := listen(host, port)?
    return Listener{handle: handle}
}

pub fn connect_stream(host str, port i32) !Conn {
    handle := connect(host, port)?
    return Conn{handle: handle}
}

pub fn (l Listener) accept() !Conn {
    handle := accept(l.handle)?
    return Conn{handle: handle}
}

pub fn (l Listener) addr() !Addr {
    return listener_addr(l.handle)
}

pub fn (l Listener) close() !void {
    close_listener(l.handle)?
    return
}

pub fn (c Conn) read(max_bytes i32) !str {
    return read(c.handle, max_bytes)
}

pub fn (c Conn) write(data str) !i32 {
    return write(c.handle, data)
}

pub fn (c Conn) close() !void {
    close(c.handle)?
    return
}

pub fn (c Conn) local_addr() !Addr {
    return local_addr(c.handle)
}

pub fn (c Conn) remote_addr() !Addr {
    return remote_addr(c.handle)
}

pub fn (c Conn) set_read_deadline(millis i32) !void {
    set_read_deadline(c.handle, millis)?
    return
}

pub fn (c Conn) set_write_deadline(millis i32) !void {
    set_write_deadline(c.handle, millis)?
    return
}
