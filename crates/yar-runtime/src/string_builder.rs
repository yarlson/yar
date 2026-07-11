use std::ptr;

use crate::{YarStr, handle_registry};

pub(crate) fn new() -> i64 {
    handle_registry::register_string_builder()
}

pub(crate) fn write(handle: i64, data: *const u8, data_len: i64) {
    let Some(handle) = handle_registry::string_builder(handle) else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
    if data_len <= 0 {
        return;
    }
    if data.is_null() {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    }

    let Ok(incoming_len) = usize::try_from(data_len) else {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    };
    let mut builder = handle.lock().unwrap_or_else(|err| err.into_inner());
    if builder.len().checked_add(incoming_len).is_none() {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    }
    builder
        .try_reserve(incoming_len)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: out of memory\n"));

    // SAFETY: data points to data_len readable bytes from the generated ABI.
    let incoming = unsafe { std::slice::from_raw_parts(data, incoming_len) };
    builder.extend_from_slice(incoming);
}

pub(crate) fn string(handle: i64) -> YarStr {
    let Some(handle) = handle_registry::string_builder(handle) else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
    let mut builder = handle.lock().unwrap_or_else(|err| err.into_inner());
    if builder.is_empty() {
        return YarStr {
            ptr: ptr::null_mut(),
            len: 0,
        };
    }

    let len = i64::try_from(builder.len())
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid string length\n"));
    let buf = super::yar_alloc(len);
    // SAFETY: buf points to builder.len() writable bytes allocated above.
    unsafe {
        ptr::copy_nonoverlapping(builder.as_ptr(), buf, builder.len());
    }
    builder.clear();
    YarStr { ptr: buf, len }
}
