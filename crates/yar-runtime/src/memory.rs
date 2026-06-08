use std::alloc::{self, Layout};
use std::mem;
use std::ptr;

const HEADER_SIZE: usize = mem::size_of::<usize>();
const ALIGN: usize = mem::align_of::<usize>();

pub(crate) fn alloc(size: i64, zeroed: bool) -> *mut u8 {
    if size < 0 {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    }

    let Ok(payload_size) = usize::try_from(size) else {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    };

    let Some(total_size) = HEADER_SIZE.checked_add(payload_size.max(1)) else {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    };

    let Ok(layout) = Layout::from_size_align(total_size, ALIGN) else {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    };

    // SAFETY: layout is constructed above. Allocation failure terminates through
    // the runtime OOM trap to match the generated-code runtime contract.
    let base = unsafe {
        if zeroed {
            alloc::alloc_zeroed(layout)
        } else {
            alloc::alloc(layout)
        }
    };
    if base.is_null() {
        super::yar_trap_oom();
    }

    // SAFETY: base points to HEADER_SIZE + payload bytes. The header is reserved
    // for future collector metadata and keeps the external pointer payload-only.
    unsafe {
        ptr::write(base.cast::<usize>(), payload_size);
        base.add(HEADER_SIZE)
    }
}
