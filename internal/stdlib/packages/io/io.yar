package io

import "conv"

pub interface Reader {
    read(max_bytes i32) !str
}

pub interface Writer {
    write(data str) !i32
}

pub interface Closer {
    close() !void
}

pub interface ReadCloser {
    read(max_bytes i32) !str
    close() !void
}

pub interface WriteCloser {
    write(data str) !i32
    close() !void
}

pub interface ReadWriter {
    read(max_bytes i32) !str
    write(data str) !i32
}

pub fn copy(dst Writer, src Reader, chunk_size i32) !i64 {
    if chunk_size <= 0 {
        return error.InvalidArgument
    }

    total := conv.to_i64(0)
    for true {
        chunk := src.read(chunk_size)?
        if len(chunk) == 0 {
            return total
        }
        written := dst.write(chunk)?
        total = total + conv.to_i64(written)
        if written != len(chunk) {
            return error.IO
        }
    }
    return total
}

pub fn read_all(src Reader, chunk_size i32, max_bytes i32) !str {
    if chunk_size <= 0 || max_bytes < 0 {
        return error.InvalidArgument
    }

    out := ""
    for true {
        chunk := src.read(chunk_size)?
        if len(chunk) == 0 {
            return out
        }
        if len(out) + len(chunk) > max_bytes {
            return error.LimitExceeded
        }
        out = out + chunk
    }
    return out
}

pub fn close_quiet(c Closer) void {
    c.close() or |err| {
    }
}
