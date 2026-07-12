use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::ptr;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crate::{YarNetAddr, YarStr, handle_registry};

const NET_OK: i32 = 0;
const NET_REFUSED: i32 = 1;
const NET_TIMEOUT: i32 = 2;
const NET_ADDR_IN_USE: i32 = 3;
const NET_CONN_RESET: i32 = 4;
const NET_NOT_FOUND: i32 = 5;
const NET_PERMISSION: i32 = 6;
const NET_INVALID_ARG: i32 = 7;
const NET_IO: i32 = 8;
const NET_CLOSED: i32 = 9;
const MAX_READ_BYTES: i32 = 64 * 1024 * 1024;
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn listen(host: YarStr, port: i32, out: *mut i64) -> i32 {
    write_out_i64(out, 0);

    let Some(host) = host_string(host, true) else {
        return NET_INVALID_ARG;
    };
    let Some(port) = port_u16(port) else {
        return NET_INVALID_ARG;
    };

    let bind_host = if host.is_empty() { "0.0.0.0" } else { &host };
    match TcpListener::bind((bind_host, port)) {
        Ok(listener) => {
            if let Err(err) = listener.set_nonblocking(true) {
                return status_from_io(err);
            }
            write_out_i64(out, handle_registry::register_listener(listener));
            NET_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn accept(raw_listener: i64, out: *mut i64) -> i32 {
    write_out_i64(out, 0);

    let Some(handle) = handle_registry::listener(raw_listener) else {
        return NET_CLOSED;
    };
    let _accept = handle.accept.lock().unwrap_or_else(|err| err.into_inner());
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }

    loop {
        match handle.listener.accept() {
            Ok((stream, _)) => {
                if handle.closed.load(Ordering::Acquire) {
                    return NET_CLOSED;
                }
                if let Err(err) = stream.set_nonblocking(false) {
                    return status_from_io(err);
                }
                write_out_i64(out, handle_registry::register_connection(stream));
                return NET_OK;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if handle.closed.load(Ordering::Acquire) {
                    return NET_CLOSED;
                }
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(err) => {
                return if handle.closed.load(Ordering::Acquire) {
                    NET_CLOSED
                } else {
                    status_from_io(err)
                };
            }
        }
    }
}

pub(crate) fn listener_addr(raw_listener: i64, out: *mut YarNetAddr) -> i32 {
    write_out_addr(out, empty_addr());

    let Some(handle) = handle_registry::listener(raw_listener) else {
        return NET_CLOSED;
    };
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }

    match handle.listener.local_addr() {
        Ok(addr) => {
            write_out_addr(out, addr_from_socket_addr(addr));
            NET_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn close_listener(raw_listener: i64) -> i32 {
    let Some(handle) = handle_registry::remove_listener(raw_listener) else {
        return NET_CLOSED;
    };
    handle.wait_for_operations();
    NET_OK
}

pub(crate) fn connect(host: YarStr, port: i32, out: *mut i64) -> i32 {
    write_out_i64(out, 0);

    let Some(host) = host_string(host, false) else {
        return NET_INVALID_ARG;
    };
    let Some(port) = port_u16(port) else {
        return NET_INVALID_ARG;
    };

    match TcpStream::connect((host.as_str(), port)) {
        Ok(stream) => {
            write_out_i64(out, handle_registry::register_connection(stream));
            NET_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn read(raw_conn: i64, max_bytes: i32, out: *mut YarStr) -> i32 {
    write_out_str(out, empty_str());
    let Some(handle) = handle_registry::connection(raw_conn) else {
        return NET_CLOSED;
    };
    if !valid_read_size(max_bytes) {
        return NET_INVALID_ARG;
    }
    let _read = handle.read.lock().unwrap_or_else(|err| err.into_inner());
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }

    let mut buffer = vec![0; max_bytes as usize];
    let mut stream = &handle.stream;
    match stream.read(&mut buffer) {
        Ok(bytes_read) => {
            if handle.closed.load(Ordering::Acquire) {
                return NET_CLOSED;
            }
            write_out_str(out, string_from_bytes(&buffer[..bytes_read]));
            NET_OK
        }
        Err(err) => status_from_connection_io(&handle, err),
    }
}

pub(crate) fn write(raw_conn: i64, data: YarStr, out: *mut i32) -> i32 {
    write_out_i32(out, 0);
    let Some(handle) = handle_registry::connection(raw_conn) else {
        return NET_CLOSED;
    };
    let Some(data) = checked_str(data, true) else {
        return NET_INVALID_ARG;
    };
    if data.len() > i32::MAX as usize {
        return NET_INVALID_ARG;
    }

    let _write = handle.write.lock().unwrap_or_else(|err| err.into_inner());
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }
    if data.is_empty() {
        return NET_OK;
    }

    let mut stream = &handle.stream;
    match write_once(&mut stream, data) {
        Ok(bytes_written) => {
            if handle.closed.load(Ordering::Acquire) {
                return NET_CLOSED;
            }
            write_out_i32(out, bytes_written as i32);
            NET_OK
        }
        Err(err) => status_from_connection_io(&handle, err),
    }
}

fn valid_read_size(max_bytes: i32) -> bool {
    (1..=MAX_READ_BYTES).contains(&max_bytes)
}

fn write_once(writer: &mut impl Write, data: &[u8]) -> io::Result<usize> {
    writer.write(data)
}

pub(crate) fn close(raw_conn: i64) -> i32 {
    let Some(handle) = handle_registry::remove_connection(raw_conn) else {
        return NET_CLOSED;
    };
    let _ = handle.stream.shutdown(Shutdown::Both);
    handle.wait_for_operations();
    NET_OK
}

pub(crate) fn local_addr(raw_conn: i64, out: *mut YarNetAddr) -> i32 {
    conn_addr(raw_conn, out, TcpStream::local_addr)
}

pub(crate) fn remote_addr(raw_conn: i64, out: *mut YarNetAddr) -> i32 {
    conn_addr(raw_conn, out, TcpStream::peer_addr)
}

pub(crate) fn set_read_deadline(raw_conn: i64, millis: i32) -> i32 {
    set_deadline(raw_conn, millis, TcpStream::set_read_timeout)
}

pub(crate) fn set_write_deadline(raw_conn: i64, millis: i32) -> i32 {
    set_deadline(raw_conn, millis, TcpStream::set_write_timeout)
}

pub(crate) fn resolve(host: YarStr, port: i32, out: *mut YarNetAddr) -> i32 {
    write_out_addr(out, empty_addr());

    let Some(host) = host_string(host, false) else {
        return NET_INVALID_ARG;
    };
    let Some(port) = port_u16(port) else {
        return NET_INVALID_ARG;
    };

    match (host.as_str(), port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => {
                write_out_addr(out, addr_from_socket_addr(addr));
                NET_OK
            }
            None => NET_NOT_FOUND,
        },
        Err(_) => NET_NOT_FOUND,
    }
}

fn conn_addr(
    raw_conn: i64,
    out: *mut YarNetAddr,
    addr_fn: fn(&TcpStream) -> io::Result<SocketAddr>,
) -> i32 {
    write_out_addr(out, empty_addr());

    let Some(handle) = handle_registry::connection(raw_conn) else {
        return NET_CLOSED;
    };
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }

    match addr_fn(&handle.stream) {
        Ok(addr) => {
            write_out_addr(out, addr_from_socket_addr(addr));
            NET_OK
        }
        Err(err) => status_from_io(err),
    }
}

fn set_deadline(
    raw_conn: i64,
    millis: i32,
    deadline_fn: fn(&TcpStream, Option<Duration>) -> io::Result<()>,
) -> i32 {
    let Some(handle) = handle_registry::connection(raw_conn) else {
        return NET_CLOSED;
    };
    if millis < 0 {
        return NET_INVALID_ARG;
    }
    if handle.closed.load(Ordering::Acquire) {
        return NET_CLOSED;
    }

    let timeout = if millis == 0 {
        None
    } else {
        Some(Duration::from_millis(millis as u64))
    };
    deadline_fn(&handle.stream, timeout).map_or_else(
        |err| status_from_connection_io(&handle, err),
        |_| {
            if handle.closed.load(Ordering::Acquire) {
                NET_CLOSED
            } else {
                NET_OK
            }
        },
    )
}

fn status_from_connection_io(handle: &handle_registry::ConnectionLease, err: io::Error) -> i32 {
    if handle.closed.load(Ordering::Acquire) {
        NET_CLOSED
    } else {
        status_from_io(err)
    }
}

fn host_string(value: YarStr, allow_empty: bool) -> Option<String> {
    let bytes = checked_str(value, allow_empty)?;
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn checked_str(value: YarStr, allow_empty: bool) -> Option<&'static [u8]> {
    if value.len < 0 || (value.ptr.is_null() && value.len != 0) {
        return None;
    }
    if !allow_empty && value.len == 0 {
        return None;
    }
    let bytes = unsafe { string_bytes(value) };
    if bytes.contains(&0) {
        return None;
    }
    Some(bytes)
}

unsafe fn string_bytes<'a>(value: YarStr) -> &'a [u8] {
    if value.len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(value.ptr.cast_const(), value.len as usize) }
}

fn port_u16(port: i32) -> Option<u16> {
    u16::try_from(port).ok()
}

fn addr_from_socket_addr(addr: SocketAddr) -> YarNetAddr {
    YarNetAddr {
        host: string_from_bytes(addr_host_bytes(addr.ip()).as_bytes()),
        port: i32::from(addr.port()),
    }
}

fn addr_host_bytes(addr: IpAddr) -> String {
    match addr {
        IpAddr::V4(addr) => addr.to_string(),
        IpAddr::V6(addr) => addr.to_string(),
    }
}

fn string_from_bytes(value: &[u8]) -> YarStr {
    if value.is_empty() {
        return empty_str();
    }

    let ptr = super::yar_alloc(value.len() as i64);
    // SAFETY: ptr points to value.len() writable bytes allocated above.
    unsafe {
        ptr::copy_nonoverlapping(value.as_ptr(), ptr, value.len());
    }
    YarStr {
        ptr,
        len: value.len() as i64,
    }
}

fn empty_str() -> YarStr {
    YarStr {
        ptr: ptr::null_mut(),
        len: 0,
    }
}

fn empty_addr() -> YarNetAddr {
    YarNetAddr {
        host: empty_str(),
        port: 0,
    }
}

fn write_out_str(out: *mut YarStr, value: YarStr) {
    if out.is_null() {
        return;
    }
    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, value);
    }
}

fn write_out_addr(out: *mut YarNetAddr, value: YarNetAddr) {
    if out.is_null() {
        return;
    }
    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, value);
    }
}

fn write_out_i32(out: *mut i32, value: i32) {
    if out.is_null() {
        return;
    }
    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, value);
    }
}

fn write_out_i64(out: *mut i64, value: i64) {
    if out.is_null() {
        return;
    }
    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, value);
    }
}

fn status_from_io(err: io::Error) -> i32 {
    match err.kind() {
        io::ErrorKind::ConnectionRefused => NET_REFUSED,
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => NET_TIMEOUT,
        io::ErrorKind::AddrInUse => NET_ADDR_IN_USE,
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe => NET_CONN_RESET,
        io::ErrorKind::NotFound | io::ErrorKind::AddrNotAvailable => NET_NOT_FOUND,
        io::ErrorKind::PermissionDenied => NET_PERMISSION,
        io::ErrorKind::InvalidInput
        | io::ErrorKind::InvalidData
        | io::ErrorKind::NotConnected
        | io::ErrorKind::Unsupported => NET_INVALID_ARG,
        _ => NET_IO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PartialWriter;

    impl Write for PartialWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            Ok(buffer.len().min(3))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn networking_write_once_reports_partial_progress() {
        assert_eq!(write_once(&mut PartialWriter, b"abcdef").unwrap(), 3);
    }

    #[test]
    fn networking_read_limit_includes_the_exact_maximum() {
        assert!(!valid_read_size(0));
        assert!(valid_read_size(MAX_READ_BYTES));
        assert!(!valid_read_size(MAX_READ_BYTES + 1));
    }
}
