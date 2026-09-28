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
    let mut state = handle.lock().unwrap_or_else(|err| err.into_inner());
    let Some(builder) = state.as_mut() else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
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
    let contents = {
        let mut state = handle.lock().unwrap_or_else(|err| err.into_inner());
        let Some(builder) = state.as_mut() else {
            super::runtime_fail(b"runtime failure: invalid string builder\n");
        };
        std::mem::take(builder)
    };
    copy_to_runtime_string(&contents)
}

pub(crate) fn finish(handle: i64) -> YarStr {
    let Some(handle) = handle_registry::remove_string_builder(handle) else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
    let contents = handle.lock().unwrap_or_else(|err| err.into_inner()).take();
    let Some(builder) = contents else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
    copy_to_runtime_string(&builder)
}

pub(crate) fn discard(handle: i64) {
    let Some(handle) = handle_registry::remove_string_builder(handle) else {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    };
    let mut state = handle.lock().unwrap_or_else(|err| err.into_inner());
    if state.take().is_none() {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    }
}

fn copy_to_runtime_string(builder: &[u8]) -> YarStr {
    if builder.is_empty() {
        return YarStr {
            ptr: ptr::null_mut(),
            len: 0,
        };
    }

    let len = i64::try_from(builder.len())
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid string length\n"));
    let buf = super::memory::alloc_bytes(builder.len());
    // SAFETY: buf points to builder.len() writable bytes allocated above.
    unsafe {
        ptr::copy_nonoverlapping(builder.as_ptr(), buf, builder.len());
    }
    YarStr { ptr: buf, len }
}
