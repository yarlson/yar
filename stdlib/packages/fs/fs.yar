package fs

pub error AlreadyExists
pub error IO
pub error InvalidArgument
pub error InvalidPath
pub error NotFound
pub error PermissionDenied

pub struct DirEntry {
    pub name str
    pub is_dir bool
}

pub enum EntryKind {
    File
    Directory
    Other
}

pub struct File {
    handle i64
}

pub fn read_file(path str) !str {
    panic("fs.read_file intrinsic")
}

pub fn write_file(path str, data str) !void {
    panic("fs.write_file intrinsic")
}

pub fn read_dir(path str) ![]DirEntry {
    panic("fs.read_dir intrinsic")
}

pub fn stat(path str) !EntryKind {
    panic("fs.stat intrinsic")
}

pub fn mkdir_all(path str) !void {
    panic("fs.mkdir_all intrinsic")
}

pub fn remove_all(path str) !void {
    panic("fs.remove_all intrinsic")
}

pub fn temp_dir(prefix str) !str {
    panic("fs.temp_dir intrinsic")
}

pub fn open_read(path str) !File {
    handle := open_read_handle(path)?
    return File{handle: handle}
}

pub fn open_write(path str) !File {
    handle := open_write_handle(path)?
    return File{handle: handle}
}

fn open_read_handle(path str) !i64 {
    panic("fs.open_read_handle intrinsic")
}

fn open_write_handle(path str) !i64 {
    panic("fs.open_write_handle intrinsic")
}

fn read_handle(handle i64, max_bytes i32) !str {
    panic("fs.read_handle intrinsic")
}

fn write_handle(handle i64, data str) !i32 {
    panic("fs.write_handle intrinsic")
}

fn close_handle(handle i64) !void {
    panic("fs.close_handle intrinsic")
}

pub fn (f File) read(max_bytes i32) !str {
    return read_handle(f.handle, max_bytes)
}

pub fn (f File) write(data str) !i32 {
    return write_handle(f.handle, data)
}

pub fn (f File) close() !void {
    close_handle(f.handle)?
    return
}
