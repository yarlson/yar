use std::collections::BTreeMap;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::YarSlice;

type TaskEntry = extern "C" fn(*mut u8, *mut u8);

struct TaskgroupHandle {
    state: Mutex<Option<TaskgroupState>>,
}

#[cfg(test)]
impl Drop for TaskgroupHandle {
    fn drop(&mut self) {
        TASKGROUP_DROPS.fetch_add(1, Ordering::SeqCst);
    }
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

static UNJOINED_TASKS: AtomicUsize = AtomicUsize::new(0);
static CHANNELS: OnceLock<Mutex<BTreeMap<usize, Arc<ChannelHandle>>>> = OnceLock::new();
#[cfg(test)]
static TASKGROUP_DROPS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn taskgroup_new(elem_size: i64) -> *mut u8 {
    if elem_size < 0 {
        super::runtime_fail(b"runtime failure: invalid taskgroup element size\n");
    }

    let handle = Box::new(TaskgroupHandle {
        state: Mutex::new(Some(TaskgroupState {
            elem_size: usize::try_from(elem_size).unwrap_or_else(|_| {
                super::runtime_fail(b"runtime failure: invalid taskgroup element size\n")
            }),
            tasks: Vec::new(),
        })),
    });
    Box::into_raw(handle).cast::<u8>()
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
    state
        .tasks
        .try_reserve(1)
        .unwrap_or_else(|_| super::yar_trap_oom());
    if !add_unjoined_tasks(&UNJOINED_TASKS, 1) {
        super::runtime_fail(b"runtime failure: task accounting exhausted\n");
    }
    let task = thread::Builder::new()
        .spawn(move || {
            let mut result = Vec::new();
            result
                .try_reserve_exact(elem_size)
                .unwrap_or_else(|_| super::yar_trap_oom());
            result.resize(elem_size, 0);
            let result_ptr = if result.is_empty() {
                ptr::null_mut()
            } else {
                result.as_mut_ptr()
            };
            entry(ctx as *mut u8, result_ptr);
            result
        })
        .unwrap_or_else(|_| {
            if !remove_unjoined_tasks(&UNJOINED_TASKS, 1) {
                super::runtime_fail(b"runtime failure: task accounting corrupted\n");
            }
            super::runtime_fail(b"runtime failure: cannot spawn task\n")
        });
    state.tasks.push(task);
}

pub(crate) fn taskgroup_wait(group: *mut u8) -> YarSlice {
    if group.is_null() {
        return empty_slice();
    }

    // SAFETY: taskgroup handles are compiler-internal, allocated by
    // taskgroup_new, and consumed exactly once by generated taskgroup code.
    let handle = unsafe { Box::from_raw(group.cast::<TaskgroupHandle>()) };
    let mut guard = handle.state.lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = guard.take() else {
        return empty_slice();
    };
    drop(guard);
    drop(handle);

    finish_taskgroup(state)
}

pub(crate) fn chan_new(elem_size: i64, capacity: i32) -> *mut u8 {
    if elem_size < 0 {
        super::runtime_fail(b"runtime failure: invalid channel element size\n");
    }
    if capacity <= 0 {
        super::runtime_fail(b"runtime failure: invalid channel capacity\n");
    }

    let elem_size = usize::try_from(elem_size).unwrap_or_else(|_| {
        super::runtime_fail(b"runtime failure: invalid channel element size\n")
    });
    let capacity = capacity as usize;
    let Some(buffer_size) = elem_size.checked_mul(capacity) else {
        super::runtime_fail(b"runtime failure: invalid channel capacity\n");
    };

    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(buffer_size)
        .unwrap_or_else(|_| super::yar_trap_oom());
    buffer.resize(buffer_size, 0);
    let channel = Arc::new(ChannelHandle {
        state: Mutex::new(ChannelState {
            elem_size,
            capacity,
            count: 0,
            head: 0,
            tail: 0,
            closed: false,
            buffer,
        }),
        can_send: Condvar::new(),
        can_recv: Condvar::new(),
    });
    let handle = super::memory::alloc_finalized(1, finalize_channel);
    let address = handle as usize;
    if channels()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(address, channel)
        .is_some()
    {
        super::runtime_fail(b"runtime failure: channel registry corrupted\n");
    }
    handle
}

pub(crate) fn chan_send(handle: *mut u8, value: *const u8) -> i32 {
    if handle.is_null() {
        return 1;
    }

    let Some(channel) = channel_from_ptr(handle) else {
        return 1;
    };
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

    let Some(channel) = channel_from_ptr(handle) else {
        return 1;
    };
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
            ptr::write_bytes(state.buffer.as_mut_ptr().add(offset), 0, state.elem_size);
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

    let Some(channel) = channel_from_ptr(handle) else {
        return;
    };
    let mut state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
    state.closed = true;
    channel.can_send.notify_all();
    channel.can_recv.notify_all();
}

/// Returns whether a validated boolean channel has been closed without
/// consuming a buffered value. Process cancellation uses channel closure as a
/// level-triggered signal so every observer sees the same state.
pub(crate) fn cancellation_requested(handle: *mut u8) -> Option<bool> {
    let channel = channel_from_ptr(handle)?;
    let state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
    (state.elem_size == size_of::<bool>()).then_some(state.closed)
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

fn channel_from_ptr(handle: *mut u8) -> Option<Arc<ChannelHandle>> {
    if handle.is_null() {
        return None;
    }
    channels()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&(handle as usize))
        .cloned()
}

fn finalize_channel(handle: *mut u8) {
    channels()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .remove(&(handle as usize));
}

fn finish_taskgroup(mut state: TaskgroupState) -> YarSlice {
    let count = state.tasks.len();
    let len = i32::try_from(count)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid taskgroup size\n"));

    let mut results = Vec::new();
    results
        .try_reserve_exact(count)
        .unwrap_or_else(|_| super::yar_trap_oom());
    for task in state.tasks.drain(..) {
        let result = task
            .join()
            .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: task panicked\n"));
        results.push(result);
    }

    let output = if state.elem_size == 0 || count == 0 {
        YarSlice {
            ptr: ptr::null_mut(),
            len,
            cap: len,
        }
    } else {
        let total_size = state
            .elem_size
            .checked_mul(count)
            .and_then(|size| i64::try_from(size).ok())
            .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid taskgroup size\n"));
        let ptr = super::yar_alloc_zeroed(total_size);
        for (idx, result) in results.iter().enumerate() {
            // SAFETY: ptr points to total_size writable bytes and each task
            // result has exactly elem_size bytes.
            unsafe {
                ptr::copy_nonoverlapping(
                    result.as_ptr(),
                    ptr.add(idx * state.elem_size),
                    state.elem_size,
                );
            }
        }
        YarSlice { ptr, len, cap: len }
    };

    if !remove_unjoined_tasks(&UNJOINED_TASKS, count) {
        super::runtime_fail(b"runtime failure: task accounting corrupted\n");
    }
    output
}

fn empty_slice() -> YarSlice {
    YarSlice {
        ptr: ptr::null_mut(),
        len: 0,
        cap: 0,
    }
}

pub(crate) fn unjoined_tasks() -> usize {
    UNJOINED_TASKS.load(Ordering::SeqCst)
}

fn add_unjoined_tasks(counter: &AtomicUsize, count: usize) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(count)
        })
        .is_ok()
}

fn remove_unjoined_tasks(counter: &AtomicUsize, count: usize) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_sub(count)
        })
        .is_ok()
}

pub(crate) fn channel_root_snapshots() -> Vec<Vec<u8>> {
    let live_channels = channels()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut roots = Vec::with_capacity(live_channels.len());
    for channel in live_channels {
        let state = channel.state.lock().unwrap_or_else(|err| err.into_inner());
        if state.elem_size == 0 || state.count == 0 {
            continue;
        }
        let mut snapshot = Vec::with_capacity(state.elem_size * state.count);
        for offset in 0..state.count {
            let slot = (state.head + offset) % state.capacity;
            let start = slot * state.elem_size;
            snapshot.extend_from_slice(&state.buffer[start..start + state.elem_size]);
        }
        roots.push(snapshot);
    }
    roots
}

fn channels() -> &'static Mutex<BTreeMap<usize, Arc<ChannelHandle>>> {
    CHANNELS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taskgroup_wait_consumes_and_reclaims_the_handle() {
        let drops_before = TASKGROUP_DROPS.load(Ordering::SeqCst);
        let handle = taskgroup_new(0);

        assert_eq!(taskgroup_wait(handle), empty_slice());
        assert_eq!(TASKGROUP_DROPS.load(Ordering::SeqCst), drops_before + 1);
    }

    #[test]
    fn task_accounting_rejects_overflow_and_underflow() {
        let counter = AtomicUsize::new(usize::MAX);
        assert!(!add_unjoined_tasks(&counter, 1));
        assert_eq!(counter.load(Ordering::SeqCst), usize::MAX);

        let counter = AtomicUsize::new(0);
        assert!(!remove_unjoined_tasks(&counter, 1));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn channel_finalization_removes_external_state_and_payload_roots() {
        let handle = chan_new(size_of::<usize>() as i64, 1);
        let payload = 0x1234_5678_usize;
        assert_eq!(chan_send(handle, payload.to_ne_bytes().as_ptr()), 0);
        assert!(snapshot_contains(payload));

        finalize_channel(handle);

        assert!(channel_from_ptr(handle).is_none());
        assert!(!snapshot_contains(payload));
    }

    #[test]
    fn closing_a_channel_wakes_a_blocked_sender() {
        use std::sync::mpsc;
        use std::time::Duration;

        let handle = chan_new(size_of::<i32>() as i64, 1);
        let first = 17_i32;
        assert_eq!(chan_send(handle, (&first as *const i32).cast()), 0);

        let address = handle as usize;
        let (result_tx, result_rx) = mpsc::channel();
        let sender = thread::spawn(move || {
            let second = 29_i32;
            result_tx
                .send(chan_send(
                    address as *mut u8,
                    (&second as *const i32).cast(),
                ))
                .unwrap();
        });
        assert!(result_rx.recv_timeout(Duration::from_millis(20)).is_err());

        chan_close(handle);

        assert_eq!(result_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 1);
        sender.join().unwrap();
    }

    #[test]
    fn cancellation_probe_validates_and_does_not_consume_the_channel() {
        let handle = chan_new(size_of::<bool>() as i64, 1);
        assert_eq!(cancellation_requested(handle), Some(false));

        let signal = true;
        assert_eq!(chan_send(handle, (&signal as *const bool).cast()), 0);
        assert_eq!(cancellation_requested(handle), Some(false));

        let channel = channel_from_ptr(handle).unwrap();
        assert_eq!(
            channel
                .state
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .count,
            1
        );

        chan_close(handle);
        assert_eq!(cancellation_requested(handle), Some(true));
        assert_eq!(cancellation_requested(ptr::null_mut()), None);

        let wrong_element_type = chan_new(size_of::<i64>() as i64, 1);
        assert_eq!(cancellation_requested(wrong_element_type), None);

        finalize_channel(handle);
        assert_eq!(cancellation_requested(handle), None);
    }

    #[test]
    fn channel_root_snapshots_include_only_live_fifo_slots() {
        let handle = chan_new(size_of::<usize>() as i64, 2);
        let first = Box::into_raw(Box::new(17_u8)) as usize;
        let second = Box::into_raw(Box::new(29_u8)) as usize;

        assert_eq!(chan_send(handle, first.to_ne_bytes().as_ptr()), 0);
        assert_eq!(chan_send(handle, second.to_ne_bytes().as_ptr()), 0);
        assert!(snapshot_contains(first));
        assert!(snapshot_contains(second));

        let mut received = 0_usize;
        assert_eq!(
            chan_recv(handle, (&mut received as *mut usize).cast::<u8>()),
            0
        );
        assert_eq!(received, first);
        assert!(!snapshot_contains(first));
        assert!(snapshot_contains(second));

        assert_eq!(
            chan_recv(handle, (&mut received as *mut usize).cast::<u8>()),
            0
        );
        assert_eq!(received, second);
        assert!(!snapshot_contains(second));

        // SAFETY: both pointers came from Box::into_raw and are reclaimed once.
        unsafe {
            drop(Box::from_raw(first as *mut u8));
            drop(Box::from_raw(second as *mut u8));
        }
    }

    fn snapshot_contains(candidate: usize) -> bool {
        let expected = candidate.to_ne_bytes();
        channel_root_snapshots().iter().any(|snapshot| {
            snapshot
                .windows(size_of::<usize>())
                .any(|window| window == expected)
        })
    }
}
