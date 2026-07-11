mod concurrency;
mod filesystem;
mod handle_registry;
mod host;
mod map;
mod memory;
mod net;
mod string;
mod string_builder;

use std::io::{self, Write};
use std::process;
use std::ptr;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YarStr {
    pub ptr: *mut u8,
    pub len: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YarSlice {
    pub ptr: *mut u8,
    pub len: i32,
    pub cap: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YarDirEntry {
    pub name: YarStr,
    pub is_dir: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YarProcessResult {
    pub exit_code: i32,
    pub stdout: YarStr,
    pub stderr: YarStr,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YarNetAddr {
    pub host: YarStr,
    pub port: i32,
}

fn write_all(mut out: impl Write, data: *const u8, len: i64) {
    if data.is_null() || len <= 0 {
        return;
    }

    let Ok(len) = usize::try_from(len) else {
        runtime_fail(b"runtime failure: invalid write length\n");
    };

    // SAFETY: The generated program passes a pointer/length pair for a live Yar
    // string. Invalid pairs are a compiler/runtime ABI violation.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let _ = out.write_all(bytes);
}

fn runtime_fail(message: &[u8]) -> ! {
    let _ = io::stderr().write_all(message);
    let _ = io::stderr().flush();
    process::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_print(data: *const u8, len: i64) {
    write_all(io::stdout(), data, len);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_eprint(data: *const u8, len: i64) {
    write_all(io::stderr(), data, len);
    let _ = io::stderr().flush();
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_panic(data: *const u8, len: i64) -> ! {
    write_all(io::stderr(), data, len);
    let _ = io::stderr().flush();
    process::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_trap_oom() -> ! {
    runtime_fail(b"runtime failure: out of memory\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_pointer_check(pointer: *const u8) {
    if pointer.is_null() {
        runtime_fail(b"runtime failure: nil pointer dereference\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_alloc(size: i64) -> *mut u8 {
    memory::alloc(size, false)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_alloc_zeroed(size: i64) -> *mut u8 {
    memory::alloc(size, true)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_gc_init_stack_top(_stack_top: *mut u8) {}

#[unsafe(no_mangle)]
pub extern "C" fn yar_gc_collect() {}

#[unsafe(no_mangle)]
pub extern "C" fn yar_taskgroup_new(elem_size: i32) -> *mut u8 {
    concurrency::taskgroup_new(elem_size)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_taskgroup_spawn(group: *mut u8, entry: *mut u8, ctx: *mut u8) {
    concurrency::taskgroup_spawn(group, entry, ctx);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_taskgroup_wait(group: *mut u8) -> YarSlice {
    concurrency::taskgroup_wait(group)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_chan_new(elem_size: i32, capacity: i32) -> *mut u8 {
    concurrency::chan_new(elem_size, capacity)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_chan_send(handle: *mut u8, value: *const u8) -> i32 {
    concurrency::chan_send(handle, value)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_chan_recv(handle: *mut u8, out: *mut u8) -> i32 {
    concurrency::chan_recv(handle, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_chan_close(handle: *mut u8) {
    concurrency::chan_close(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_set_args(argc: i32, argv: *mut *mut std::ffi::c_char) {
    host::set_args(argc, argv);
}

fn check_integer_divrem(dividend: i64, divisor: i64, min: i64) {
    if divisor == 0 {
        runtime_fail(b"runtime failure: integer division or remainder by zero\n");
    }
    if dividend == min && divisor == -1 {
        runtime_fail(b"runtime failure: integer division or remainder overflow\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_i32_divrem_check(dividend: i32, divisor: i32) {
    check_integer_divrem(i64::from(dividend), i64::from(divisor), i64::from(i32::MIN));
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_i64_divrem_check(dividend: i64, divisor: i64) {
    check_integer_divrem(dividend, divisor, i64::MIN);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_array_index_check(index: i64, len: i64) {
    if index < 0 || index >= len {
        runtime_fail(b"runtime failure: array index out of range\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_slice_index_check(index: i64, len: i64) {
    if index < 0 || index >= len {
        runtime_fail(b"runtime failure: slice index out of range\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_slice_range_check(start: i64, end: i64, len: i64) {
    if start < 0 || end < start || end > len {
        runtime_fail(b"runtime failure: slice range out of bounds\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_str_index_check(index: i64, len: i64) {
    if index < 0 || index >= len {
        runtime_fail(b"runtime failure: string index out of range\n");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_str_equal(a_ptr: *const u8, a_len: i64, b_ptr: *const u8, b_len: i64) -> i32 {
    string::equal(a_ptr, a_len, b_ptr, b_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_str_concat(
    a_ptr: *const u8,
    a_len: i64,
    b_ptr: *const u8,
    b_len: i64,
) -> YarStr {
    string::concat(a_ptr, a_len, b_ptr, b_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_str_from_byte(value: i32) -> YarStr {
    if !(0..=255).contains(&value) {
        runtime_fail(b"runtime failure: byte value out of range\n");
    }

    let ptr = yar_alloc(1);
    // SAFETY: yar_alloc returned one writable byte or terminated.
    unsafe {
        ptr::write(ptr, value as u8);
    }
    YarStr { ptr, len: 1 }
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_to_str_i32(value: i32) -> YarStr {
    string::from_owned(value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_to_str_i64(value: i64) -> YarStr {
    string::from_owned(value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_new(key_kind: i32, key_size: i32, value_size: i32) -> *mut u8 {
    map::new(key_kind, key_size, value_size)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_set(map_ptr: *mut u8, key: *const u8, value: *const u8) {
    map::set(map_ptr, key, value);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_get(map_ptr: *mut u8, key: *const u8, value_out: *mut u8) -> i32 {
    map::get(map_ptr, key, value_out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_has(map_ptr: *mut u8, key: *const u8) -> i32 {
    map::has(map_ptr, key)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_delete(map_ptr: *mut u8, key: *const u8) {
    map::delete(map_ptr, key);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_len(map_ptr: *mut u8) -> i32 {
    map::len(map_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_map_keys(map_ptr: *mut u8) -> YarSlice {
    map::keys(map_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_sb_new() -> i64 {
    string_builder::new()
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_sb_write(handle: i64, data: *const u8, data_len: i64) {
    string_builder::write(handle, data, data_len);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_sb_string(handle: i64) -> YarStr {
    string_builder::string(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_process_args(out: *mut YarSlice) {
    host::process_args(out);
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_env_lookup(name: YarStr, out: *mut YarStr) -> i32 {
    host::env_lookup(name, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_process_run(argv: *const YarSlice, out: *mut YarProcessResult) -> i32 {
    host::process_run(argv, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_process_run_inherit(argv: *const YarSlice, out: *mut i32) -> i32 {
    host::process_run_inherit(argv, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_read_file(path: YarStr, out: *mut YarStr) -> i32 {
    filesystem::read_file(path, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_write_file(path: YarStr, data: YarStr) -> i32 {
    filesystem::write_file(path, data)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_read_dir(path: YarStr, out: *mut YarSlice) -> i32 {
    filesystem::read_dir(path, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_stat(path: YarStr, kind_out: *mut i32) -> i32 {
    filesystem::stat(path, kind_out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_mkdir_all(path: YarStr) -> i32 {
    filesystem::mkdir_all(path)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_remove_all(path: YarStr) -> i32 {
    filesystem::remove_all(path)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_temp_dir(prefix: YarStr, out: *mut YarStr) -> i32 {
    filesystem::temp_dir(prefix, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_open_read(path: YarStr, out: *mut i64) -> i32 {
    filesystem::open_read(path, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_open_write(path: YarStr, out: *mut i64) -> i32 {
    filesystem::open_write(path, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_read_handle(handle: i64, max_bytes: i32, out: *mut YarStr) -> i32 {
    filesystem::read_handle(handle, max_bytes, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_write_handle(handle: i64, data: YarStr, out: *mut i32) -> i32 {
    filesystem::write_handle(handle, data, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_fs_close_handle(handle: i64) -> i32 {
    filesystem::close_handle(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_listen(host: YarStr, port: i32, out: *mut i64) -> i32 {
    net::listen(host, port, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_accept(listener: i64, out: *mut i64) -> i32 {
    net::accept(listener, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_listener_addr(listener: i64, out: *mut YarNetAddr) -> i32 {
    net::listener_addr(listener, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_close_listener(listener: i64) -> i32 {
    net::close_listener(listener)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_connect(host: YarStr, port: i32, out: *mut i64) -> i32 {
    net::connect(host, port, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_read(conn: i64, max_bytes: i32, out: *mut YarStr) -> i32 {
    net::read(conn, max_bytes, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_write(conn: i64, data: YarStr, out: *mut i32) -> i32 {
    net::write(conn, data, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_close(conn: i64) -> i32 {
    net::close(conn)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_local_addr(conn: i64, out: *mut YarNetAddr) -> i32 {
    net::local_addr(conn, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_remote_addr(conn: i64, out: *mut YarNetAddr) -> i32 {
    net::remote_addr(conn, out)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_set_read_deadline(conn: i64, millis: i32) -> i32 {
    net::set_read_deadline(conn, millis)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_set_write_deadline(conn: i64, millis: i32) -> i32 {
    net::set_write_deadline(conn, millis)
}

#[unsafe(no_mangle)]
pub extern "C" fn yar_net_resolve(host: YarStr, port: i32, out: *mut YarNetAddr) -> i32 {
    net::resolve(host, port, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn str_from_runtime(value: YarStr) -> String {
        if value.len == 0 {
            return String::new();
        }
        assert!(!value.ptr.is_null());
        let bytes = unsafe { std::slice::from_raw_parts(value.ptr, value.len as usize) };
        String::from_utf8(bytes.to_vec()).expect("runtime string should be valid UTF-8")
    }

    fn runtime_slice(values: &[&str]) -> YarSlice {
        let total = values
            .len()
            .checked_mul(size_of::<YarStr>())
            .and_then(|size| i64::try_from(size).ok())
            .expect("test slice size should fit");
        let ptr = yar_alloc_zeroed(total).cast::<YarStr>();
        for (idx, value) in values.iter().enumerate() {
            unsafe {
                ptr::write(ptr.add(idx), string::from_owned((*value).to_owned()));
            }
        }
        YarSlice {
            ptr: ptr.cast::<u8>(),
            len: values.len() as i32,
            cap: values.len() as i32,
        }
    }

    #[test]
    fn zeroed_allocation_returns_writable_zeroed_memory() {
        let ptr = yar_alloc_zeroed(8);
        assert!(!ptr.is_null());

        let bytes = unsafe { std::slice::from_raw_parts_mut(ptr, 8) };
        assert_eq!(bytes, &[0; 8]);
        bytes[0] = 42;
        assert_eq!(bytes[0], 42);
    }

    #[test]
    fn string_helpers_match_generated_code_runtime_contract() {
        let a = b"yar";
        let b = b"lang";

        assert_eq!(
            yar_str_equal(a.as_ptr(), a.len() as i64, a.as_ptr(), a.len() as i64),
            1
        );
        assert_eq!(
            yar_str_equal(a.as_ptr(), a.len() as i64, b.as_ptr(), b.len() as i64),
            0
        );

        let combined = yar_str_concat(a.as_ptr(), a.len() as i64, b.as_ptr(), b.len() as i64);
        assert_eq!(str_from_runtime(combined), "yarlang");
    }

    #[test]
    fn conversion_helpers_return_runtime_strings() {
        assert_eq!(str_from_runtime(yar_to_str_i32(-32)), "-32");
        assert_eq!(
            str_from_runtime(yar_to_str_i64(9_000_000_000)),
            "9000000000"
        );
        assert_eq!(str_from_runtime(yar_str_from_byte(65)), "A");
    }

    #[test]
    fn string_builder_returns_accumulated_string_and_resets() {
        let handle = yar_sb_new();
        assert_ne!(handle, 0);

        yar_sb_write(handle, b"hello".as_ptr(), 5);
        yar_sb_write(handle, b", yar".as_ptr(), 5);

        assert_eq!(str_from_runtime(yar_sb_string(handle)), "hello, yar");
        assert_eq!(str_from_runtime(yar_sb_string(handle)), "");
    }

    #[test]
    fn string_builder_serializes_concurrent_writes() {
        let handle = yar_sb_new();
        let writers = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..128 {
                        yar_sb_write(handle, b"x".as_ptr(), 1);
                    }
                })
            })
            .collect::<Vec<_>>();

        for writer in writers {
            writer.join().expect("string builder writer should finish");
        }

        let value = str_from_runtime(yar_sb_string(handle));
        assert_eq!(value.len(), 1024);
        assert!(value.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn map_helpers_support_i32_keys() {
        let handle = yar_map_new(1, 4, 4);
        assert!(!handle.is_null());

        let key = 7_i32;
        let value = 42_i32;
        yar_map_set(
            handle,
            (&key as *const i32).cast::<u8>(),
            (&value as *const i32).cast::<u8>(),
        );

        let mut out = 0_i32;
        assert_eq!(
            yar_map_get(
                handle,
                (&key as *const i32).cast::<u8>(),
                (&mut out as *mut i32).cast::<u8>(),
            ),
            1
        );
        assert_eq!(out, 42);
        assert_eq!(yar_map_has(handle, (&key as *const i32).cast::<u8>()), 1);
        assert_eq!(yar_map_len(handle), 1);

        let keys = yar_map_keys(handle);
        assert_eq!(keys.len, 1);
        assert_eq!(keys.cap, 1);
        let copied_key = unsafe { *(keys.ptr.cast::<i32>()) };
        assert_eq!(copied_key, key);

        yar_map_delete(handle, (&key as *const i32).cast::<u8>());
        assert_eq!(yar_map_len(handle), 0);
        assert_eq!(yar_map_has(handle, (&key as *const i32).cast::<u8>()), 0);
    }

    #[test]
    fn map_helpers_support_string_keys_and_growth() {
        let handle = yar_map_new(3, 16, 4);
        assert!(!handle.is_null());

        let mut keys = Vec::new();
        for i in 0..12_i32 {
            let text = format!("key-{i}");
            let yar_key = string::from_owned(text);
            yar_map_set(
                handle,
                (&yar_key as *const YarStr).cast::<u8>(),
                (&i as *const i32).cast::<u8>(),
            );
            keys.push((yar_key, i));
        }

        assert_eq!(yar_map_len(handle), 12);
        for (key, value) in keys {
            let mut out = 0_i32;
            assert_eq!(
                yar_map_get(
                    handle,
                    (&key as *const YarStr).cast::<u8>(),
                    (&mut out as *mut i32).cast::<u8>(),
                ),
                1
            );
            assert_eq!(out, value);
        }
    }

    #[test]
    fn process_args_returns_runtime_string_slice() {
        let first = CString::new("yar").expect("valid c string");
        let second = CString::new("run").expect("valid c string");
        let mut argv = [first.as_ptr().cast_mut(), second.as_ptr().cast_mut()];

        yar_set_args(argv.len() as i32, argv.as_mut_ptr());

        let mut out = YarSlice {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
        };
        yar_process_args(&mut out);

        assert_eq!(out.len, 2);
        assert_eq!(out.cap, 2);
        let args =
            unsafe { std::slice::from_raw_parts(out.ptr.cast::<YarStr>(), out.len as usize) };
        assert_eq!(str_from_runtime(args[0]), "yar");
        assert_eq!(str_from_runtime(args[1]), "run");
    }

    #[test]
    fn env_lookup_returns_runtime_string() {
        unsafe {
            std::env::set_var("YAR_RUNTIME_TEST_ENV", "ok");
        }

        let name = string::from_owned("YAR_RUNTIME_TEST_ENV".to_owned());
        let mut out = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };

        assert_eq!(yar_env_lookup(name, &mut out), 0);
        assert_eq!(str_from_runtime(out), "ok");

        let missing = string::from_owned("YAR_RUNTIME_TEST_ENV_MISSING".to_owned());
        assert_eq!(yar_env_lookup(missing, &mut out), 1);
    }

    #[test]
    fn filesystem_file_directory_and_stat_helpers_work() {
        let mut temp_dir = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            yar_fs_temp_dir(string::from_owned("yar-runtime-".to_owned()), &mut temp_dir),
            0
        );

        let root = str_from_runtime(temp_dir);
        let nested = format!("{root}/nested");
        assert_eq!(yar_fs_mkdir_all(string::from_owned(nested.clone())), 0);

        let file_path = format!("{nested}/data.txt");
        assert_eq!(
            yar_fs_write_file(
                string::from_owned(file_path.clone()),
                string::from_owned("hello fs".to_owned()),
            ),
            0
        );

        let mut file_text = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            yar_fs_read_file(string::from_owned(file_path.clone()), &mut file_text),
            0
        );
        assert_eq!(str_from_runtime(file_text), "hello fs");

        let mut kind = -1;
        assert_eq!(yar_fs_stat(string::from_owned(file_path), &mut kind), 0);
        assert_eq!(kind, 0);
        assert_eq!(
            yar_fs_stat(string::from_owned(nested.clone()), &mut kind),
            0
        );
        assert_eq!(kind, 1);

        let mut entries = YarSlice {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
        };
        assert_eq!(yar_fs_read_dir(string::from_owned(nested), &mut entries), 0);
        assert_eq!(entries.len, 1);
        let entries = unsafe {
            std::slice::from_raw_parts(entries.ptr.cast::<YarDirEntry>(), entries.len as usize)
        };
        assert_eq!(str_from_runtime(entries[0].name), "data.txt");
        assert_eq!(entries[0].is_dir, 0);

        assert_eq!(yar_fs_remove_all(string::from_owned(root)), 0);
    }

    #[test]
    fn filesystem_stream_handles_work_and_report_closed() {
        let mut temp_dir = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            yar_fs_temp_dir(string::from_owned("yar-stream-".to_owned()), &mut temp_dir),
            0
        );

        let root = str_from_runtime(temp_dir);
        let file_path = format!("{root}/stream.txt");
        let mut handle = 0_i64;
        assert_eq!(
            yar_fs_open_write(string::from_owned(file_path.clone()), &mut handle),
            0
        );
        assert_ne!(handle, 0);

        let mut written = 0_i32;
        let invalid_data = YarStr {
            ptr: std::ptr::null_mut(),
            len: 1,
        };
        assert_eq!(yar_fs_write_handle(handle, invalid_data, &mut written), 6);
        assert_eq!(written, 0);
        assert_eq!(
            yar_fs_write_handle(
                handle,
                string::from_owned("abcdef".to_owned()),
                &mut written
            ),
            0
        );
        assert_eq!(written, 6);
        assert_eq!(yar_fs_close_handle(handle), 0);
        assert_eq!(yar_fs_close_handle(handle), 7);
        let closed_handle = handle;

        assert_eq!(
            yar_fs_open_read(string::from_owned(file_path), &mut handle),
            0
        );
        assert_ne!(handle, closed_handle);
        let mut chunk = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(yar_fs_read_handle(closed_handle, 3, &mut chunk), 7);
        assert_eq!(yar_fs_read_handle(handle, 0, &mut chunk), 6);
        assert_eq!(yar_fs_read_handle(handle, 3, &mut chunk), 0);
        assert_eq!(str_from_runtime(chunk), "abc");
        assert_eq!(yar_fs_close_handle(handle), 0);

        assert_eq!(yar_fs_remove_all(string::from_owned(root)), 0);
    }

    #[test]
    fn filesystem_stream_handle_serializes_concurrent_writes() {
        let mut temp_dir = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            yar_fs_temp_dir(
                string::from_owned("yar-stream-concurrent-".to_owned()),
                &mut temp_dir
            ),
            0
        );

        let root = str_from_runtime(temp_dir);
        let file_path = format!("{root}/stream.txt");
        let mut handle = 0_i64;
        assert_eq!(
            yar_fs_open_write(string::from_owned(file_path.clone()), &mut handle),
            0
        );

        let writers = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    let byte = YarStr {
                        ptr: b"x".as_ptr().cast_mut(),
                        len: 1,
                    };
                    for _ in 0..128 {
                        let mut written = 0_i32;
                        assert_eq!(yar_fs_write_handle(handle, byte, &mut written), 0);
                        assert_eq!(written, 1);
                    }
                })
            })
            .collect::<Vec<_>>();

        for writer in writers {
            writer.join().expect("filesystem writer should finish");
        }
        assert_eq!(yar_fs_close_handle(handle), 0);

        let contents = std::fs::read(&file_path).expect("stream file should be readable");
        assert_eq!(contents.len(), 1024);
        assert!(contents.iter().all(|byte| *byte == b'x'));
        assert_eq!(yar_fs_remove_all(string::from_owned(root)), 0);
    }

    #[test]
    fn networking_helpers_support_loopback_tcp_flow() {
        let mut listener = 0_i64;
        assert_eq!(
            yar_net_listen(string::from_owned("127.0.0.1".to_owned()), 0, &mut listener),
            0
        );
        assert_ne!(listener, 0);

        let mut listener_addr = YarNetAddr {
            host: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            port: 0,
        };
        assert_eq!(yar_net_listener_addr(listener, &mut listener_addr), 0);
        assert_eq!(str_from_runtime(listener_addr.host), "127.0.0.1");
        assert!(listener_addr.port > 0);

        let mut client = 0_i64;
        assert_eq!(
            yar_net_connect(
                string::from_owned("127.0.0.1".to_owned()),
                listener_addr.port,
                &mut client,
            ),
            0
        );
        assert_ne!(client, 0);

        let mut server = 0_i64;
        assert_eq!(yar_net_accept(listener, &mut server), 0);
        assert_ne!(server, 0);

        let mut written = 0_i32;
        let invalid_data = YarStr {
            ptr: std::ptr::null_mut(),
            len: 1,
        };
        let empty_data = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        let mut invalid_read = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(yar_net_read(server, 0, &mut invalid_read), 7);
        assert_eq!(yar_net_write(client, invalid_data, &mut written), 7);
        assert_eq!(written, 0);
        assert_eq!(yar_net_write(client, empty_data, &mut written), 0);
        assert_eq!(written, 0);
        assert_eq!(yar_net_set_read_deadline(client, -1), 7);
        assert_eq!(
            yar_net_write(client, string::from_owned("hello".to_owned()), &mut written),
            0
        );
        assert_eq!(written, 5);

        let mut received = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(yar_net_read(server, 4096, &mut received), 0);
        assert_eq!(str_from_runtime(received), "hello");

        assert_eq!(
            yar_net_write(server, string::from_owned("world".to_owned()), &mut written),
            0
        );
        assert_eq!(written, 5);

        assert_eq!(yar_net_read(client, 4096, &mut received), 0);
        assert_eq!(str_from_runtime(received), "world");

        let mut remote_addr = YarNetAddr {
            host: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            port: 0,
        };
        let mut local_addr = remote_addr;
        assert_eq!(yar_net_remote_addr(server, &mut remote_addr), 0);
        assert_eq!(yar_net_local_addr(client, &mut local_addr), 0);
        assert_eq!(remote_addr.port, local_addr.port);

        assert_eq!(yar_net_set_read_deadline(client, 50), 0);
        assert_eq!(yar_net_set_write_deadline(client, 50), 0);
        assert_eq!(yar_net_set_read_deadline(client, 0), 0);
        assert_eq!(yar_net_set_write_deadline(client, 0), 0);

        let mut resolved = YarNetAddr {
            host: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            port: 0,
        };
        assert_eq!(
            yar_net_resolve(
                string::from_owned("127.0.0.1".to_owned()),
                80,
                &mut resolved
            ),
            0
        );
        assert_eq!(str_from_runtime(resolved.host), "127.0.0.1");
        assert_eq!(resolved.port, 80);

        assert_eq!(yar_net_close(client), 0);
        assert_eq!(yar_net_close(client), 9);
        assert_eq!(yar_net_close(server), 0);
        assert_eq!(yar_net_close_listener(listener), 0);
        assert_eq!(yar_net_close_listener(listener), 9);
    }

    #[test]
    fn networking_helpers_reject_invalid_arguments() {
        let mut handle = -1_i64;
        assert_eq!(
            yar_net_listen(string::from_owned("127.0.0.1".to_owned()), -1, &mut handle),
            7
        );
        assert_eq!(handle, 0);

        assert_eq!(
            yar_net_connect(
                YarStr {
                    ptr: std::ptr::null_mut(),
                    len: 0,
                },
                80,
                &mut handle,
            ),
            7
        );

        let mut out = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(yar_net_read(0, 1, &mut out), 9);
        assert_eq!(yar_net_read(handle, 0, &mut out), 9);
        assert_eq!(yar_net_set_read_deadline(handle, -1), 9);
    }

    #[test]
    fn opaque_handle_abis_reject_unissued_ids() {
        let data = YarStr {
            ptr: b"x".as_ptr().cast_mut(),
            len: 1,
        };
        let invalid_data = YarStr {
            ptr: std::ptr::null_mut(),
            len: 1,
        };
        let empty_data = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };

        for handle in [0, -1, i64::MAX] {
            let mut text = YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            };
            let mut written = -1_i32;
            let mut addr = YarNetAddr {
                host: YarStr {
                    ptr: std::ptr::null_mut(),
                    len: 0,
                },
                port: 0,
            };

            assert_eq!(yar_fs_read_handle(handle, 1, &mut text), 7);
            assert_eq!(yar_fs_read_handle(handle, 0, &mut text), 7);
            assert_eq!(yar_fs_write_handle(handle, data, &mut written), 7);
            assert_eq!(yar_fs_write_handle(handle, invalid_data, &mut written), 7);
            assert_eq!(written, 0);
            assert_eq!(yar_fs_close_handle(handle), 7);

            assert_eq!(yar_net_listener_addr(handle, &mut addr), 9);
            assert_eq!(yar_net_close_listener(handle), 9);
            assert_eq!(yar_net_read(handle, 1, &mut text), 9);
            assert_eq!(yar_net_read(handle, 0, &mut text), 9);
            assert_eq!(yar_net_write(handle, data, &mut written), 9);
            assert_eq!(yar_net_write(handle, invalid_data, &mut written), 9);
            assert_eq!(yar_net_write(handle, empty_data, &mut written), 9);
            assert_eq!(written, 0);
            assert_eq!(yar_net_set_read_deadline(handle, -1), 9);
            assert_eq!(yar_net_set_write_deadline(handle, -1), 9);
            assert_eq!(yar_net_close(handle), 9);
        }
    }

    #[test]
    fn wrong_kind_lookups_do_not_consume_live_resources() {
        let mut temp_dir = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            yar_fs_temp_dir(
                string::from_owned("yar-handle-kinds-".to_owned()),
                &mut temp_dir
            ),
            0
        );
        let root = str_from_runtime(temp_dir);
        let file_path = format!("{root}/data.txt");
        let mut file = 0_i64;
        assert_eq!(
            yar_fs_open_write(string::from_owned(file_path), &mut file),
            0
        );

        let builder = yar_sb_new();
        let mut listener = 0_i64;
        assert_eq!(
            yar_net_listen(string::from_owned("127.0.0.1".to_owned()), 0, &mut listener),
            0
        );
        let mut addr = YarNetAddr {
            host: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            port: 0,
        };
        assert_eq!(yar_net_listener_addr(listener, &mut addr), 0);

        let mut client = 0_i64;
        assert_eq!(
            yar_net_connect(
                string::from_owned("127.0.0.1".to_owned()),
                addr.port,
                &mut client,
            ),
            0
        );
        let mut server = 0_i64;
        assert_eq!(yar_net_accept(listener, &mut server), 0);

        let data = YarStr {
            ptr: b"ok".as_ptr().cast_mut(),
            len: 2,
        };
        let mut text = YarStr {
            ptr: std::ptr::null_mut(),
            len: 0,
        };
        let mut written = 0_i32;

        assert_eq!(yar_fs_close_handle(listener), 7);
        assert_eq!(yar_fs_close_handle(builder), 7);
        assert_eq!(yar_net_close(file), 9);
        assert_eq!(yar_net_close(builder), 9);
        assert_eq!(yar_net_close_listener(client), 9);
        assert_eq!(yar_net_read(listener, 1, &mut text), 9);
        assert_eq!(yar_net_listener_addr(client, &mut addr), 9);

        assert_eq!(yar_fs_write_handle(file, data, &mut written), 0);
        assert_eq!(written, 2);
        yar_sb_write(builder, b"builder".as_ptr(), 7);
        assert_eq!(str_from_runtime(yar_sb_string(builder)), "builder");
        assert_eq!(yar_net_listener_addr(listener, &mut addr), 0);
        assert_eq!(yar_net_write(client, data, &mut written), 0);
        assert_eq!(written, 2);
        assert_eq!(yar_net_read(server, 2, &mut text), 0);
        assert_eq!(str_from_runtime(text), "ok");

        assert_eq!(yar_fs_close_handle(file), 0);
        assert_eq!(yar_net_close(client), 0);
        assert_eq!(yar_net_close(server), 0);
        assert_eq!(yar_net_close_listener(listener), 0);
        assert_eq!(yar_fs_remove_all(string::from_owned(root)), 0);
    }

    extern "C" fn task_square(ctx: *mut u8, result: *mut u8) {
        let value = unsafe { *(ctx.cast::<i32>()) };
        unsafe {
            ptr::write(result.cast::<i32>(), value * value);
        }
    }

    extern "C" fn task_void_count(ctx: *mut u8, _result: *mut u8) {
        let counter = unsafe { &*(ctx.cast::<std::sync::atomic::AtomicI32>()) };
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn taskgroup_helpers_run_tasks_and_preserve_spawn_order() {
        let group = yar_taskgroup_new(size_of::<i32>() as i32);
        assert!(!group.is_null());

        let mut first = 2_i32;
        let mut second = 3_i32;
        yar_taskgroup_spawn(
            group,
            task_square as *mut u8,
            (&mut first as *mut i32).cast::<u8>(),
        );
        yar_taskgroup_spawn(
            group,
            task_square as *mut u8,
            (&mut second as *mut i32).cast::<u8>(),
        );

        let out = yar_taskgroup_wait(group);
        assert_eq!(out.len, 2);
        assert_eq!(out.cap, 2);
        assert!(!out.ptr.is_null());
        let values = unsafe { std::slice::from_raw_parts(out.ptr.cast::<i32>(), out.len as usize) };
        assert_eq!(values, &[4, 9]);
    }

    #[test]
    fn taskgroup_helpers_support_void_results() {
        let group = yar_taskgroup_new(0);
        let counter = std::sync::atomic::AtomicI32::new(0);

        yar_taskgroup_spawn(
            group,
            task_void_count as *mut u8,
            (&counter as *const std::sync::atomic::AtomicI32)
                .cast_mut()
                .cast::<u8>(),
        );
        yar_taskgroup_spawn(
            group,
            task_void_count as *mut u8,
            (&counter as *const std::sync::atomic::AtomicI32)
                .cast_mut()
                .cast::<u8>(),
        );

        let out = yar_taskgroup_wait(group);
        assert!(out.ptr.is_null());
        assert_eq!(out.len, 2);
        assert_eq!(out.cap, 2);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn channel_helpers_are_fifo_and_report_closed() {
        let handle = yar_chan_new(size_of::<i32>() as i32, 2);
        assert!(!handle.is_null());

        let first = 7_i32;
        let second = 11_i32;
        assert_eq!(
            yar_chan_send(handle, (&first as *const i32).cast::<u8>()),
            0
        );
        assert_eq!(
            yar_chan_send(handle, (&second as *const i32).cast::<u8>()),
            0
        );

        let mut out = 0_i32;
        assert_eq!(
            yar_chan_recv(handle, (&mut out as *mut i32).cast::<u8>()),
            0
        );
        assert_eq!(out, first);
        assert_eq!(
            yar_chan_recv(handle, (&mut out as *mut i32).cast::<u8>()),
            0
        );
        assert_eq!(out, second);

        yar_chan_close(handle);
        assert_eq!(
            yar_chan_send(handle, (&first as *const i32).cast::<u8>()),
            1
        );
        assert_eq!(
            yar_chan_recv(handle, (&mut out as *mut i32).cast::<u8>()),
            1
        );
        assert_eq!(
            yar_chan_send(std::ptr::null_mut(), (&first as *const i32).cast::<u8>()),
            1
        );
        assert_eq!(
            yar_chan_recv(std::ptr::null_mut(), (&mut out as *mut i32).cast::<u8>()),
            1
        );

        let handle = yar_chan_new(size_of::<i32>() as i32, 1);
        assert_eq!(
            yar_chan_send(handle, (&first as *const i32).cast::<u8>()),
            0
        );
        yar_chan_close(handle);
        assert_eq!(
            yar_chan_recv(handle, (&mut out as *mut i32).cast::<u8>()),
            0
        );
        assert_eq!(out, first);
        assert_eq!(
            yar_chan_recv(handle, (&mut out as *mut i32).cast::<u8>()),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_run_captures_output_and_status() {
        let argv = runtime_slice(&[
            "/bin/sh",
            "-c",
            "printf 'captured stdout\\n'; printf 'captured stderr\\n' >&2; exit 7",
        ]);
        let mut out = YarProcessResult {
            exit_code: 0,
            stdout: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            stderr: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
        };

        assert_eq!(yar_process_run(&argv, &mut out), 0);
        assert_eq!(out.exit_code, 7);
        assert_eq!(str_from_runtime(out.stdout), "captured stdout\n");
        assert_eq!(str_from_runtime(out.stderr), "captured stderr\n");
    }

    #[cfg(unix)]
    #[test]
    fn process_run_inherit_reports_exit_status() {
        let argv = runtime_slice(&["/bin/sh", "-c", "exit 3"]);
        let mut exit_code = 0_i32;

        assert_eq!(yar_process_run_inherit(&argv, &mut exit_code), 0);
        assert_eq!(exit_code, 3);
    }

    #[test]
    fn process_run_rejects_empty_argv() {
        let argv = runtime_slice(&[]);
        let mut out = YarProcessResult {
            exit_code: 0,
            stdout: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
            stderr: YarStr {
                ptr: std::ptr::null_mut(),
                len: 0,
            },
        };

        assert_eq!(yar_process_run(&argv, &mut out), 3);
    }
}
