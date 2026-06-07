package main

import "fs"
import "io"
import "path"

fn main() !i32 {
    dir := fs.temp_dir("yar-io")?
    src_path := path.join([]str{dir, "src.txt"})
    dst_path := path.join([]str{dir, "dst.txt"})

    fs.write_file(src_path, "hello streaming world")?

    src := fs.open_read(src_path)?
    dst := fs.open_write(dst_path)?
    copied := io.copy(dst, src, 5)?
    src.close()?
    dst.close()?

    if copied != 21 {
        print("copy count mismatch\n")
        return 1
    }

    copied_data := fs.read_file(dst_path)?
    if copied_data != "hello streaming world" {
        print("copy data mismatch\n")
        return 1
    }

    read_src := fs.open_read(dst_path)?
    all := io.read_all(read_src, 4, 100)?
    read_src.close()?

    if all != "hello streaming world" {
        print("read_all mismatch\n")
        return 1
    }

    expect_closed_read(read_src)?
    expect_invalid_copy(dst, read_src)?

    limited_src := fs.open_read(dst_path)?
    expect_limit_exceeded(limited_src)?
    limited_src.close()?

    fs.remove_all(dir)?
    print("io ok\n")
    return 0
}

fn expect_closed_read(f fs.File) !void {
    data := f.read(4) or |err| {
        if err == error.Closed {
            return
        }
        return error.IO
    }
    if data == "" {
        return error.IO
    }
    return error.IO
}

fn expect_invalid_copy(dst io.Writer, src io.Reader) !void {
    copied := io.copy(dst, src, 0) or |err| {
        if err == error.InvalidArgument {
            return
        }
        return error.IO
    }
    if copied == 0 {
        return error.IO
    }
    return error.IO
}

fn expect_limit_exceeded(src io.Reader) !void {
    data := io.read_all(src, 4, 3) or |err| {
        if err == error.LimitExceeded {
            return
        }
        return error.IO
    }
    if data == "" {
        return error.IO
    }
    return error.IO
}
