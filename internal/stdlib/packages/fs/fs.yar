package fs

pub struct DirEntry {
    name str
    is_dir bool
}

pub enum EntryKind {
    File
    Directory
    Other
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
