use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{YarDirEntry, YarSlice, YarStr};

const FS_OK: i32 = 0;
const FS_NOT_FOUND: i32 = 1;
const FS_PERMISSION_DENIED: i32 = 2;
const FS_ALREADY_EXISTS: i32 = 3;
const FS_INVALID_PATH: i32 = 4;
const FS_IO: i32 = 5;
const FS_INVALID_ARGUMENT: i32 = 6;
const FS_CLOSED: i32 = 7;

const KIND_FILE: i32 = 0;
const KIND_DIRECTORY: i32 = 1;
const KIND_OTHER: i32 = 2;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct FileHandle {
    file: Option<File>,
}

pub(crate) fn read_file(path: YarStr, out: *mut YarStr) -> i32 {
    write_out_str(out, empty_str());
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };

    match fs::read(path) {
        Ok(bytes) => {
            write_out_str(out, string_from_bytes(&bytes));
            FS_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn write_file(path: YarStr, data: YarStr) -> i32 {
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };
    let Some(data) = checked_str(data) else {
        return FS_INVALID_ARGUMENT;
    };

    match fs::write(path, data) {
        Ok(()) => FS_OK,
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn read_dir(path: YarStr, out: *mut YarSlice) -> i32 {
    write_out_slice(out, empty_slice());
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) => return status_from_io(err),
    };

    let mut result = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => return status_from_io(err),
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => return status_from_io(err),
        };
        result.push(YarDirEntry {
            name: string_from_bytes(&os_string_bytes(entry.file_name())),
            is_dir: u8::from(file_type.is_dir()),
        });
    }

    if result.is_empty() {
        return FS_OK;
    }

    let total = result
        .len()
        .checked_mul(size_of::<YarDirEntry>())
        .and_then(|size| i64::try_from(size).ok())
        .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid directory size\n"));
    let ptr = super::yar_alloc(total).cast::<YarDirEntry>();
    for (idx, entry) in result.into_iter().enumerate() {
        // SAFETY: ptr points to result.len() YarDirEntry slots allocated above.
        unsafe {
            ptr::write(ptr.add(idx), entry);
        }
    }
    let len = i32::try_from(total as usize / size_of::<YarDirEntry>())
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid directory size\n"));
    write_out_slice(
        out,
        YarSlice {
            ptr: ptr.cast::<u8>(),
            len,
            cap: len,
        },
    );
    FS_OK
}

pub(crate) fn stat(path: YarStr, kind_out: *mut i32) -> i32 {
    write_out_i32(kind_out, KIND_OTHER);
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };

    match fs::metadata(path) {
        Ok(metadata) => {
            let kind = if metadata.is_file() {
                KIND_FILE
            } else if metadata.is_dir() {
                KIND_DIRECTORY
            } else {
                KIND_OTHER
            };
            write_out_i32(kind_out, kind);
            FS_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn mkdir_all(path: YarStr) -> i32 {
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };
    if path.as_os_str().is_empty() {
        return FS_INVALID_PATH;
    }

    match fs::create_dir_all(path) {
        Ok(()) => FS_OK,
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn remove_all(path: YarStr) -> i32 {
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };

    match fs::metadata(&path) {
        Ok(metadata) => {
            let result = if metadata.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            result.map_or_else(status_from_io, |_| FS_OK)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => FS_OK,
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn temp_dir(prefix: YarStr, out: *mut YarStr) -> i32 {
    write_out_str(out, empty_str());
    let Some(prefix) = checked_str(prefix) else {
        return FS_INVALID_PATH;
    };
    if prefix
        .iter()
        .any(|byte| matches!(*byte, b'\0' | b'/' | b'\\'))
    {
        return FS_INVALID_PATH;
    }

    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    for _ in 0..100 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut name = os_string_from_bytes(prefix.to_vec());
        name.push(format!("{pid}-{nanos}-{counter}"));
        let path = base.join(name);
        match fs::create_dir(&path) {
            Ok(()) => {
                write_out_str(out, string_from_path(path));
                return FS_OK;
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return status_from_io(err),
        }
    }

    FS_ALREADY_EXISTS
}

pub(crate) fn open_read(path: YarStr, out: *mut i64) -> i32 {
    open(path, out, OpenMode::Read)
}

pub(crate) fn open_write(path: YarStr, out: *mut i64) -> i32 {
    open(path, out, OpenMode::Write)
}

pub(crate) fn read_handle(raw_handle: i64, max_bytes: i32, out: *mut YarStr) -> i32 {
    write_out_str(out, empty_str());
    if max_bytes <= 0 {
        return FS_INVALID_ARGUMENT;
    }

    let Some(handle) = handle_from_i64(raw_handle) else {
        return FS_CLOSED;
    };
    let Some(file) = handle.file.as_mut() else {
        return FS_CLOSED;
    };

    let mut buffer = vec![0; max_bytes as usize];
    match file.read(&mut buffer) {
        Ok(bytes_read) => {
            write_out_str(out, string_from_bytes(&buffer[..bytes_read]));
            FS_OK
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn write_handle(raw_handle: i64, data: YarStr, out: *mut i32) -> i32 {
    write_out_i32(out, 0);
    let Some(data) = checked_str(data) else {
        return FS_INVALID_ARGUMENT;
    };
    if data.len() > i32::MAX as usize {
        return FS_INVALID_ARGUMENT;
    }

    let Some(handle) = handle_from_i64(raw_handle) else {
        return FS_CLOSED;
    };
    let Some(file) = handle.file.as_mut() else {
        return FS_CLOSED;
    };

    match file.write(data) {
        Ok(bytes_written) => {
            write_out_i32(out, bytes_written as i32);
            if bytes_written == data.len() {
                FS_OK
            } else {
                FS_IO
            }
        }
        Err(err) => status_from_io(err),
    }
}

pub(crate) fn close_handle(raw_handle: i64) -> i32 {
    let Some(handle) = handle_from_i64(raw_handle) else {
        return FS_CLOSED;
    };
    let Some(file) = handle.file.take() else {
        return FS_CLOSED;
    };

    match file.sync_all() {
        Ok(()) => FS_OK,
        Err(err) => status_from_io(err),
    }
}

enum OpenMode {
    Read,
    Write,
}

fn open(path: YarStr, out: *mut i64, mode: OpenMode) -> i32 {
    write_out_i64(out, 0);
    let Ok(path) = path_from_yar(path) else {
        return FS_INVALID_PATH;
    };

    let file = match mode {
        OpenMode::Read => File::open(path),
        OpenMode::Write => OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path),
    };

    match file {
        Ok(file) => {
            let handle = Box::leak(Box::new(FileHandle { file: Some(file) }));
            write_out_i64(out, handle as *mut FileHandle as i64);
            FS_OK
        }
        Err(err) => status_from_io(err),
    }
}

fn handle_from_i64(raw_handle: i64) -> Option<&'static mut FileHandle> {
    if raw_handle == 0 {
        return None;
    }

    let ptr = raw_handle as *mut FileHandle;
    if ptr.is_null() {
        return None;
    }

    // SAFETY: handles are created by open() using Box::leak and remain valid for
    // the process lifetime, matching the runtime ABI's opaque handle.
    Some(unsafe { &mut *ptr })
}

fn path_from_yar(value: YarStr) -> Result<PathBuf, ()> {
    let Some(bytes) = checked_str(value) else {
        return Err(());
    };
    Ok(PathBuf::from(os_string_from_bytes(bytes.to_vec())))
}

fn checked_str(value: YarStr) -> Option<&'static [u8]> {
    if value.len < 0 || (value.ptr.is_null() && value.len != 0) {
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

fn string_from_path(path: PathBuf) -> YarStr {
    string_from_bytes(&os_string_bytes(path.into_os_string()))
}

fn empty_str() -> YarStr {
    YarStr {
        ptr: ptr::null_mut(),
        len: 0,
    }
}

fn empty_slice() -> YarSlice {
    YarSlice {
        ptr: ptr::null_mut(),
        len: 0,
        cap: 0,
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

fn write_out_slice(out: *mut YarSlice, value: YarSlice) {
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
        io::ErrorKind::NotFound => FS_NOT_FOUND,
        io::ErrorKind::PermissionDenied => FS_PERMISSION_DENIED,
        io::ErrorKind::AlreadyExists => FS_ALREADY_EXISTS,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => FS_INVALID_PATH,
        _ => FS_IO,
    }
}

#[cfg(unix)]
fn os_string_from_bytes(value: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(value)
}

#[cfg(not(unix))]
fn os_string_from_bytes(value: Vec<u8>) -> OsString {
    OsString::from(String::from_utf8_lossy(&value).into_owned())
}

#[cfg(unix)]
fn os_string_bytes(value: OsString) -> Vec<u8> {
    use std::os::unix::ffi::OsStringExt;

    value.into_vec()
}

#[cfg(not(unix))]
fn os_string_bytes(value: OsString) -> Vec<u8> {
    value.to_string_lossy().into_owned().into_bytes()
}
