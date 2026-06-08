use std::ptr;
use std::sync::{Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::YarSlice;

type TaskEntry = extern "C" fn(*mut u8, *mut u8);

struct TaskgroupHandle {
    state: Mutex<Option<TaskgroupState>>,
}

struct TaskgroupState {
    elem_size: usize,
    tasks: Vec<JoinHandle<Vec<u8>>>,
}

struct ChannelHandle {
    state: Mutex<ChannelState>,
    can_send: Condvar,
    can_recv: Condvar,
}

struct ChannelState {
    elem_size: usize,
    capacity: usize,
    count: usize,
    head: usize,
    tail: usize,
    closed: bool,
    buffer: Vec<u8>,
}

pub(crate) fn taskgroup_new(elem_size: i32) -> *mut u8 {
    if elem_size < 0 {
        super::runtime_fail(b"runtime failure: invalid taskgroup element size\n");
    }

    let handle = Box::leak(Box::new(TaskgroupHandle {
        state: Mutex::new(Some(TaskgroupState {
            elem_size: elem_size as usize,
            tasks: Vec::new(),
        })),
    }));
    (handle as *mut TaskgroupHandle).cast::<u8>()
}

pub(crate) fn taskgroup_spawn(group: *mut u8, entry: *mut u8, ctx: *mut u8) {
    if group.is_null() || entry.is_null() {
        super::runtime_fail(b"runtime failure: invalid taskgroup spawn\n");
    }

    let handle = taskgroup_from_ptr(group);
    let mut state = handle.state.lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = state.as_mut() else {
        super::runtime_fail(b"runtime failure: invalid taskgroup spawn\n");
    };

    // SAFETY: codegen passes a function pointer with the yar_task_entry ABI as
    // an opaque ptr, matching the generated-code runtime ABI boundary.
    let entry: TaskEntry = unsafe { std::mem::transmute(entry) };
    let ctx = ctx as usize;
    let elem_size = state.elem_size;
    let task = thread::spawn(move || {
        let mut result = vec![0; elem_size];
        let result_ptr = if result.is_empty() {
            ptr::null_mut()
        } else {
            result.as_mut_ptr()
        };
        entry(ctx as *mut u8, result_ptr);
        result
    });
    state.tasks.push(task);
}

pub(crate) fn taskgroup_wait(group: *mut u8) -> YarSlice {
    if group.is_null() {
        return empty_slice();
    }

    let handle = taskgroup_from_ptr(group);
    let mut guard = handle.state.lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = guard.take() else {
        return empty_slice();
    };
    drop(guard);

    finish_taskgroup(state)
}

pub(crate) fn chan_new(elem_size: i32, capacity: i32) -> *mut u8 {
    if elem_size < 0 || capacity <= 0 {
        super::runtime_fail(b"runtime failure: invalid channel capacity\n");
    }

    let elem_size = elem_size as usize;
    let capacity = capacity as usize;
    let Some(buffer_size) = elem_size.checked_mul(capacity) else {
        super::runtime_fail(b"runtime failure: invalid channel capacity\n");
    };

    let handle = Box::leak(Box::new(ChannelHandle {
        state: Mutex::new(ChannelState {
            elem_size,
            capacity,
            count: 0,
            head: 0,
            tail: 0,
            closed: false,
            buffer: vec![0; buffer_size],
        }),
        can_send: Condvar::new(),
        can_recv: Condvar::new(),
    }));
    (handle as *mut ChannelHandle).cast::<u8>()
}

pub(crate) fn chan_send(handle: *mut u8, value: *const u8) -> i32 {
    if handle.is_null() {
        return 1;
    }

    let channel = channel_from_ptr(handle);
    let mut state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
    while !state.closed && state.count == state.capacity {
        state = channel
            .can_send
            .wait(state)
            .unwrap_or_else(|err| err.into_inner());
    }
    if state.closed {
        return 1;
    }

    if state.elem_size > 0 {
        if value.is_null() {
            return 1;
        }
        let offset = state.tail * state.elem_size;
        // SAFETY: the generated caller provides elem_size initialized bytes.
        unsafe {
            ptr::copy_nonoverlapping(
                value,
                state.buffer.as_mut_ptr().add(offset),
                state.elem_size,
            );
        }
    }
    state.tail = (state.tail + 1) % state.capacity;
    state.count += 1;
    channel.can_recv.notify_one();
    0
}

pub(crate) fn chan_recv(handle: *mut u8, out: *mut u8) -> i32 {
    if handle.is_null() {
        return 1;
    }

    let channel = channel_from_ptr(handle);
    let mut state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
    while state.count == 0 && !state.closed {
        state = channel
            .can_recv
            .wait(state)
            .unwrap_or_else(|err| err.into_inner());
    }
    if state.count == 0 && state.closed {
        return 1;
    }

    if state.elem_size > 0 {
        if out.is_null() {
            return 1;
        }
        let offset = state.head * state.elem_size;
        // SAFETY: out points to elem_size writable bytes by compiler/runtime ABI.
        unsafe {
            ptr::copy_nonoverlapping(state.buffer.as_ptr().add(offset), out, state.elem_size);
        }
    }
    state.head = (state.head + 1) % state.capacity;
    state.count -= 1;
    channel.can_send.notify_one();
    0
}

pub(crate) fn chan_close(handle: *mut u8) {
    if handle.is_null() {
        return;
    }

    let channel = channel_from_ptr(handle);
    let mut state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
    state.closed = true;
    channel.can_send.notify_all();
    channel.can_recv.notify_all();
}

fn taskgroup_from_ptr<'a>(group: *mut u8) -> &'a TaskgroupHandle {
    let ptr = group.cast::<TaskgroupHandle>();
    if ptr.is_null() {
        super::runtime_fail(b"runtime failure: invalid taskgroup spawn\n");
    }

    // SAFETY: taskgroup handles are created by taskgroup_new with Box::leak and
    // remain valid for the process lifetime.
    unsafe { &*ptr }
}

fn channel_from_ptr<'a>(handle: *mut u8) -> &'a ChannelHandle {
    let ptr = handle.cast::<ChannelHandle>();
    if ptr.is_null() {
        super::runtime_fail(b"runtime failure: invalid channel handle\n");
    }

    // SAFETY: channel handles are created by chan_new with Box::leak and remain
    // valid for the process lifetime.
    unsafe { &*ptr }
}

fn finish_taskgroup(mut state: TaskgroupState) -> YarSlice {
    let count = state.tasks.len();
    let len = i32::try_from(count)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid taskgroup size\n"));

    let mut results = Vec::with_capacity(count);
    for task in state.tasks.drain(..) {
        let result = task
            .join()
            .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: task panicked\n"));
        results.push(result);
    }

    if state.elem_size == 0 || count == 0 {
        return YarSlice {
            ptr: ptr::null_mut(),
            len,
            cap: len,
        };
    }

    let total_size = state
        .elem_size
        .checked_mul(count)
        .and_then(|size| i64::try_from(size).ok())
        .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid taskgroup size\n"));
    let ptr = super::yar_alloc_zeroed(total_size);
    for (idx, result) in results.iter().enumerate() {
        // SAFETY: ptr points to total_size writable bytes and each task result
        // has exactly elem_size bytes.
        unsafe {
            ptr::copy_nonoverlapping(
                result.as_ptr(),
                ptr.add(idx * state.elem_size),
                state.elem_size,
            );
        }
    }

    YarSlice { ptr, len, cap: len }
}

fn empty_slice() -> YarSlice {
    YarSlice {
        ptr: ptr::null_mut(),
        len: 0,
        cap: 0,
    }
}
