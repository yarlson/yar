use std::ptr;

use crate::YarStr;

#[repr(C)]
struct StringBuilder {
    buf: *mut u8,
    len: i64,
    cap: i64,
}

pub(crate) fn new() -> *mut u8 {
    let handle = super::yar_alloc_zeroed(size_of::<StringBuilder>() as i64).cast::<StringBuilder>();
    // SAFETY: handle points to writable runtime-managed memory sized for StringBuilder.
    unsafe {
        (*handle).cap = 64;
        (*handle).buf = super::yar_alloc((*handle).cap);
    }
    handle.cast::<u8>()
}

pub(crate) fn write(handle: *mut u8, data: *const u8, data_len: i64) {
    if data_len <= 0 {
        return;
    }
    if handle.is_null() || data.is_null() {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    }

    let Ok(incoming_len) = usize::try_from(data_len) else {
        super::runtime_fail(b"runtime failure: invalid string length\n");
    };

    let sb = handle.cast::<StringBuilder>();
    // SAFETY: handle is a string builder created by yar_sb_new.
    unsafe {
        let Some(needed) = (*sb).len.checked_add(data_len) else {
            super::runtime_fail(b"runtime failure: invalid string length\n");
        };
        if needed > (*sb).cap {
            let mut new_cap = (*sb).cap.saturating_mul(2);
            if new_cap < needed {
                new_cap = needed;
            }
            let new_buf = super::yar_alloc(new_cap);
            if (*sb).len > 0 {
                ptr::copy_nonoverlapping((*sb).buf, new_buf, (*sb).len as usize);
            }
            (*sb).buf = new_buf;
            (*sb).cap = new_cap;
        }

        ptr::copy_nonoverlapping(data, (*sb).buf.add((*sb).len as usize), incoming_len);
        (*sb).len = needed;
    }
}

pub(crate) fn string(handle: *mut u8) -> YarStr {
    if handle.is_null() {
        super::runtime_fail(b"runtime failure: invalid string builder\n");
    }

    let sb = handle.cast::<StringBuilder>();
    // SAFETY: handle is a string builder created by yar_sb_new.
    unsafe {
        if (*sb).len == 0 {
            return YarStr {
                ptr: ptr::null_mut(),
                len: 0,
            };
        }

        let buf = super::yar_alloc((*sb).len);
        ptr::copy_nonoverlapping((*sb).buf, buf, (*sb).len as usize);
        let result = YarStr {
            ptr: buf,
            len: (*sb).len,
        };
        (*sb).len = 0;
        result
    }
}
