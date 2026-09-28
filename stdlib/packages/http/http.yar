package http

import "std/net"

pub error BadRequest
pub error BodyTooLarge
pub error ExpectationFailed
pub error HeaderNotFound
pub error HeaderTooLarge
pub error HTTPVersionNotSupported
pub error InvalidArgument
pub error InvalidResponse
pub error PathValueNotFound
pub error RouteConflict
pub error URITooLong
pub error UnsupportedMethod
pub error UnsupportedTransferEncoding

pub struct Limits {
    max_head_bytes i32
    max_body_bytes i32
    read_timeout_millis i32
    write_timeout_millis i32
}

pub struct Server {
    listener net.Listener
    limits Limits
}

pub struct Connection {
    conn net.Conn
    limits Limits
}

pub fn default_limits() Limits {
    return Limits{
        max_head_bytes: 32768,
        max_body_bytes: 1048576,
        read_timeout_millis: 5000,
        write_timeout_millis: 5000,
    }
}

pub fn limits(
    max_head_bytes i32,
    max_body_bytes i32,
    read_timeout_millis i32,
    write_timeout_millis i32,
) !Limits {
    if max_head_bytes < 1024 || max_head_bytes > 1048576 {
        return error.InvalidArgument
    }
    if max_body_bytes < 0 || max_body_bytes > 67108864 {
        return error.InvalidArgument
    }
    if read_timeout_millis < 1 || read_timeout_millis > 3600000 {
        return error.InvalidArgument
    }
    if write_timeout_millis < 1 || write_timeout_millis > 3600000 {
        return error.InvalidArgument
    }
    return Limits{
        max_head_bytes: max_head_bytes,
        max_body_bytes: max_body_bytes,
        read_timeout_millis: read_timeout_millis,
        write_timeout_millis: write_timeout_millis,
    }
}

pub fn listen(addr net.Addr, limits Limits) !Server {
    listener := net.listen_stream(addr.host, addr.port)?
    return Server{listener: listener, limits: limits}
}

pub fn (s Server) accept() !Connection {
    conn := s.listener.accept()?
    return Connection{conn: conn, limits: s.limits}
}

pub fn (s Server) addr() !net.Addr {
    return s.listener.addr()
}

pub fn (s Server) close() !void {
    s.listener.close()?
    return
}

pub fn (c Connection) close() !void {
    c.conn.close()?
    return
}

pub fn (c Connection) local_addr() !net.Addr {
    return c.conn.local_addr()
}

pub fn (c Connection) remote_addr() !net.Addr {
    return c.conn.remote_addr()
}

pub fn (c Connection) serve(handler fn(Request) !Response) !void {
    c.conn.set_read_deadline_after(c.limits.read_timeout_millis) or |err| {
        close_quiet(c.conn)
        err?
        return
    }
    req := read_request(c.conn, c.limits) or |err| {
        if protocol_error(err) {
            write_error_response_quiet(
                c.conn,
                protocol_status(err),
                c.limits.write_timeout_millis,
            )
            drain_before_close(c.conn, c.limits)
        }
        close_quiet(c.conn)
        err?
        return
    }

    resp := handler(req) or |err| {
        write_error_response_quiet(c.conn, 500, c.limits.write_timeout_millis)
        drain_before_close(c.conn, c.limits)
        close_quiet(c.conn)
        err?
        return
    }

    c.conn.set_write_deadline_after(c.limits.write_timeout_millis) or |err| {
        close_quiet(c.conn)
        err?
        return
    }
    write_response(c.conn, req.method, resp, c.limits.max_head_bytes) or |err| {
        if err == error.InvalidResponse {
            write_error_response_quiet(c.conn, 500, c.limits.write_timeout_millis)
            drain_before_close(c.conn, c.limits)
        }
        close_quiet(c.conn)
        err?
        return
    }
    c.conn.shutdown_write() or |err| {
        close_quiet(c.conn)
        err?
        return
    }
    drain_before_close(c.conn, c.limits)
    c.conn.close()?
    return
}

fn close_quiet(conn net.Conn) void {
    conn.close() or |err| {
        return
    }
}

fn write_error_response_quiet(conn net.Conn, status i32, timeout_millis i32) void {
    conn.set_write_deadline_after(timeout_millis) or |err| {
        return
    }
    write_error_response(conn, status) or |err| {
        return
    }
    conn.shutdown_write() or |err| {
        return
    }
}

fn drain_before_close(conn net.Conn, limits Limits) void {
    timeout := limits.read_timeout_millis
    if timeout > 100 {
        timeout = 100
    }
    conn.set_read_deadline_after(timeout) or |err| {
        return
    }

    drained := 0
    for drained < 65536 {
        read_size := 4096
        if 65536 - drained < read_size {
            read_size = 65536 - drained
        }
        chunk := conn.read(read_size) or |err| {
            return
        }
        if len(chunk) == 0 {
            return
        }
        drained += len(chunk)
    }
}

fn protocol_error(err error) bool {
    return err == error.BadRequest ||
        err == error.BodyTooLarge ||
        err == error.ExpectationFailed ||
        err == error.HeaderTooLarge ||
        err == error.HTTPVersionNotSupported ||
        err == error.URITooLong ||
        err == error.UnsupportedMethod ||
        err == error.UnsupportedTransferEncoding
}

fn protocol_status(err error) i32 {
    if err == error.BodyTooLarge {
        return 413
    }
    if err == error.ExpectationFailed {
        return 417
    }
    if err == error.HeaderTooLarge {
        return 431
    }
    if err == error.URITooLong {
        return 414
    }
    if err == error.UnsupportedMethod || err == error.UnsupportedTransferEncoding {
        return 501
    }
    if err == error.HTTPVersionNotSupported {
        return 505
    }
    return 400
}
