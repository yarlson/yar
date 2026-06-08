use std::ptr;

use crate::YarStr;

pub(crate) fn equal(a_ptr: *const u8, a_len: i64, b_ptr: *const u8, b_len: i64) -> i32 {
    if a_len != b_len {
        return 0;
    }

    let Some(a) = (unsafe { bytes(a_ptr, a_len) }) else {
        return 0;
    };
    let Some(b) = (unsafe { bytes(b_ptr, b_len) }) else {
        return 0;
    };

    i32::from(a == b)
}

pub(crate) fn concat(a_ptr: *const u8, a_len: i64, b_ptr: *const u8, b_len: i64) -> YarStr {
    let Some(a) = (unsafe { bytes(a_ptr, a_len) }) else {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    };
    let Some(b) = (unsafe { bytes(b_ptr, b_len) }) else {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    };

    let Some(len) = a.len().checked_add(b.len()) else {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    };

    if len == 0 {
        return YarStr {
            ptr: ptr::null_mut(),
            len: 0,
        };
    }

    let ptr = super::yar_alloc(len as i64);
    // SAFETY: ptr points to len writable bytes allocated above. The source
    // slices are live for the duration of this call.
    unsafe {
        ptr::copy_nonoverlapping(a.as_ptr(), ptr, a.len());
        ptr::copy_nonoverlapping(b.as_ptr(), ptr.add(a.len()), b.len());
    }

    YarStr {
        ptr,
        len: len as i64,
    }
}

pub(crate) fn from_owned(value: String) -> YarStr {
    if value.is_empty() {
        return YarStr {
            ptr: ptr::null_mut(),
            len: 0,
        };
    }

    let bytes = value.as_bytes();
    let ptr = super::yar_alloc(bytes.len() as i64);
    // SAFETY: ptr points to bytes.len() writable bytes allocated above.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    YarStr {
        ptr,
        len: bytes.len() as i64,
    }
}

unsafe fn bytes<'a>(ptr: *const u8, len: i64) -> Option<&'a [u8]> {
    if len < 0 {
        return None;
    }

    if len == 0 {
        return Some(&[]);
    }

    if ptr.is_null() {
        return None;
    }

    let len = usize::try_from(len).ok()?;
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}
