mod heap;
mod mutators;

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

pub(crate) use mutators::{blocking, register_current_thread, safepoint, wait};

const DEFAULT_MINIMUM_BUDGET: usize = 4 * 1024 * 1024;

static CONFIGURE: Once = Once::new();
static MINIMUM_BUDGET: OnceLock<usize> = OnceLock::new();
static ROOTS: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());

#[repr(C)]
pub struct Descriptor {
    stride: u64,
    count: u64,
}

#[repr(C)]
pub(crate) struct StaticDescriptor<const N: usize> {
    stride: u64,
    count: u64,
    offsets: [u64; N],
}

impl<const N: usize> StaticDescriptor<N> {
    pub(crate) const fn new(stride: usize, offsets: [u64; N]) -> Self {
        Self {
            stride: stride as u64,
            count: N as u64,
            offsets,
        }
    }

    pub(crate) fn as_descriptor(&'static self) -> *const Descriptor {
        (self as *const Self).cast()
    }
}

pub(crate) static EVERY_WORD: StaticDescriptor<1> = StaticDescriptor::new(size_of::<usize>(), [0]);

type RangeVisitor = extern "C" fn(usize, usize, *mut c_void);

unsafe extern "C" {
    fn yar_gc_for_each_readable_range(
        low: usize,
        high: usize,
        visitor: RangeVisitor,
        context: *mut c_void,
    );
}

pub(crate) fn init_main_thread(stack_top: *mut u8) {
    std::mem::forget(register_current_thread(stack_top));
}

pub(crate) fn alloc(size: i64, descriptor: *const Descriptor) -> *mut u8 {
    if size < 0 {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    }
    let size = usize::try_from(size)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid allocation size\n"));
    allocate(size, descriptor)
}

pub(crate) fn alloc_bytes(size: usize) -> *mut u8 {
    allocate(size, std::ptr::null())
}

pub(crate) fn alloc_words(size: usize) -> *mut u8 {
    allocate(size, EVERY_WORD.as_descriptor())
}

pub(crate) fn alloc_finalized(
    size: usize,
    descriptor: *const Descriptor,
    finalizer: fn(*mut u8),
) -> *mut u8 {
    let object = allocate(size, descriptor);
    heap::register_finalizer(object, finalizer);
    object
}

pub(crate) fn add_root(object: *mut u8) {
    *lock_roots().entry(object as usize).or_insert(0) += 1;
}

pub(crate) fn remove_root(object: *mut u8) {
    let mut roots = lock_roots();
    let Some(count) = roots.get_mut(&(object as usize)) else {
        super::runtime_fail(b"runtime failure: collector root registry corrupted\n");
    };
    *count -= 1;
    if *count == 0 {
        roots.remove(&(object as usize));
    }
}

pub(crate) fn collect() {
    mutators::stop_the_world(|stacks| {
        let mut collection = heap::Collection::begin();
        for stack in stacks {
            scan_stack(&mut collection, stack.low, stack.high);
        }
        for &root in lock_roots().keys() {
            collection.add_root(root);
        }
        collection.finish(minimum_budget());
    });
}

fn allocate(size: usize, descriptor: *const Descriptor) -> *mut u8 {
    CONFIGURE.call_once(|| heap::set_minimum_budget(minimum_budget()));
    safepoint();
    if heap::collection_due() {
        collect();
    }
    if let Some(object) = heap::allocate(size, descriptor) {
        return object;
    }
    collect();
    heap::allocate(size, descriptor).unwrap_or_else(|| super::yar_trap_oom())
}

fn scan_stack(collection: &mut heap::Collection<'_>, low: usize, high: usize) {
    extern "C" fn visit(low: usize, high: usize, context: *mut c_void) {
        // SAFETY: scan_stack passes a live Collection for this synchronous call.
        let collection = unsafe { &mut *context.cast::<heap::Collection<'_>>() };
        collection.add_conservative_range(low, high);
    }
    // SAFETY: [low, high) is the stopped thread's live stack; the shim visits
    // only readable subranges synchronously.
    unsafe {
        yar_gc_for_each_readable_range(
            low,
            high,
            visit,
            (collection as *mut heap::Collection<'_>).cast(),
        );
    }
}

fn minimum_budget() -> usize {
    *MINIMUM_BUDGET.get_or_init(configured_minimum_budget)
}

fn configured_minimum_budget() -> usize {
    std::env::var("YAR_GC_HEAP_TARGET_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(DEFAULT_MINIMUM_BUDGET)
}

fn lock_roots() -> MutexGuard<'static, BTreeMap<usize, usize>> {
    ROOTS.lock().unwrap_or_else(|err| err.into_inner())
}

impl Descriptor {
    unsafe fn layout(descriptor: *const Self) -> (usize, &'static [u64]) {
        // SAFETY: the caller passes a complete descriptor; its offsets
        // immediately follow the two header words in the repr(C) layout.
        unsafe {
            let stride = (*descriptor).stride as usize;
            let count = (*descriptor).count as usize;
            let offsets = descriptor.cast::<u64>().add(2);
            (stride, std::slice::from_raw_parts(offsets, count))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar};

    const PROBE_ENV: &str = "YAR_GC_TEST_PROBE";
    static FINALIZED: AtomicUsize = AtomicUsize::new(0);
    static SECOND_WORD: StaticDescriptor<1> = StaticDescriptor::new(16, [8]);

    fn run_isolated(probe: &str) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", probe, "--nocapture"])
            .env(PROBE_ENV, probe)
            .env("YAR_GC_HEAP_TARGET_BYTES", "1024")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "stdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn is_probe(probe: &str) -> bool {
        std::env::var(PROBE_ENV).is_ok_and(|value| value == probe)
    }

    fn collect_from(roots: &[*mut u8]) {
        let mut collection = heap::Collection::begin();
        for &root in roots {
            collection.add_root(root as usize);
        }
        collection.finish(1);
    }

    fn store_word(object: *mut u8, offset: usize, value: *mut u8) {
        // SAFETY: tests store into words inside objects they allocated.
        unsafe { object.add(offset).cast::<usize>().write(value as usize) };
    }

    fn count_finalization(_object: *mut u8) {
        FINALIZED.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn reclaims_unrooted_objects_and_keeps_transitively_reachable_ones() {
        run_isolated("memory::tests::reachability_probe");
    }

    #[test]
    fn reachability_probe() {
        if !is_probe("memory::tests::reachability_probe") {
            return;
        }
        let parent = alloc_words(32);
        let child = alloc_bytes(8);
        let garbage = alloc_bytes(8);
        store_word(parent, 8, child);

        collect_from(&[parent]);

        assert!(heap::is_live(parent as usize));
        assert!(heap::is_live(child as usize));
        assert!(!heap::is_live(garbage as usize));
    }

    #[test]
    fn interior_pointers_retain_their_enclosing_object() {
        run_isolated("memory::tests::interior_pointer_probe");
    }

    #[test]
    fn interior_pointer_probe() {
        if !is_probe("memory::tests::interior_pointer_probe") {
            return;
        }
        let small = alloc_bytes(64);
        let large = alloc_bytes(3 * heap::PAGE_SIZE);

        // SAFETY: both offsets lie inside the allocated objects.
        let interior = unsafe { [small.add(63), large.add(2 * heap::PAGE_SIZE + 5)] };
        collect_from(&interior);

        assert!(heap::is_live(small as usize));
        assert!(heap::is_live(large as usize));
    }

    #[test]
    fn descriptors_limit_tracing_to_declared_pointer_words() {
        run_isolated("memory::tests::descriptor_probe");
    }

    #[test]
    fn descriptor_probe() {
        if !is_probe("memory::tests::descriptor_probe") {
            return;
        }
        let elements = allocate(32, SECOND_WORD.as_descriptor());
        let ignored = alloc_bytes(8);
        let first_traced = alloc_bytes(8);
        let second_traced = alloc_bytes(8);
        store_word(elements, 0, ignored);
        store_word(elements, 8, first_traced);
        store_word(elements, 24, second_traced);
        let bytes = alloc_bytes(8);
        let behind_bytes = alloc_bytes(8);
        store_word(bytes, 0, behind_bytes);

        collect_from(&[elements, bytes]);

        assert!(heap::is_live(first_traced as usize));
        assert!(heap::is_live(second_traced as usize));
        assert!(!heap::is_live(ignored as usize));
        assert!(!heap::is_live(behind_bytes as usize));
    }

    #[test]
    fn finalizers_run_once_when_their_object_becomes_unreachable() {
        run_isolated("memory::tests::finalizer_probe");
    }

    #[test]
    fn finalizer_probe() {
        if !is_probe("memory::tests::finalizer_probe") {
            return;
        }
        let kept = alloc_finalized(8, ptr::null(), count_finalization);
        alloc_finalized(8, ptr::null(), count_finalization);

        collect_from(&[kept]);
        assert_eq!(FINALIZED.load(Ordering::SeqCst), 1);
        collect_from(&[kept]);
        assert_eq!(FINALIZED.load(Ordering::SeqCst), 1);

        collect_from(&[]);
        assert_eq!(FINALIZED.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn reclaimed_slots_are_reused_zeroed() {
        run_isolated("memory::tests::reuse_probe");
    }

    #[test]
    fn reuse_probe() {
        if !is_probe("memory::tests::reuse_probe") {
            return;
        }
        let first = alloc_bytes(16);
        // SAFETY: first points to 16 writable bytes.
        unsafe { first.write_bytes(0xAB, 16) };
        collect_from(&[]);

        let reused = alloc_bytes(16);

        assert_eq!(reused, first);
        // SAFETY: reused points to 16 readable bytes.
        assert_eq!(unsafe { std::slice::from_raw_parts(reused, 16) }, &[0; 16]);
    }

    #[test]
    fn stop_the_world_scans_the_stacks_of_blocked_threads() {
        run_isolated("memory::tests::blocked_thread_probe");
    }

    #[test]
    fn blocked_thread_probe() {
        if !is_probe("memory::tests::blocked_thread_probe") {
            return;
        }
        let mut stack_top = 0_u8;
        init_main_thread(&mut stack_top);
        let signal = Arc::new((Mutex::new(0_usize), Condvar::new()));
        let worker_signal = Arc::clone(&signal);
        let worker = std::thread::spawn(move || {
            let mut worker_top = 0_u8;
            let _registration = register_current_thread(std::hint::black_box(&mut worker_top));
            let object = std::hint::black_box(alloc_bytes(8));
            let (lock, changed) = &*worker_signal;
            let mut state = lock.lock().unwrap();
            *state = object as usize;
            changed.notify_all();
            while *state != 0 {
                state = wait(changed, lock, state);
            }
            std::hint::black_box(object);
        });
        let (lock, changed) = &*signal;
        let object = {
            let mut state = lock.lock().unwrap();
            while *state == 0 {
                state = changed.wait(state).unwrap();
            }
            let object = *state;
            *state = 1;
            object
        };

        collect();

        assert!(heap::is_live(object));
        *lock.lock().unwrap() = 0;
        changed.notify_all();
        worker.join().unwrap();
    }

    #[test]
    fn allocation_pressure_triggers_collection() {
        run_isolated("memory::tests::allocation_pressure_probe");
    }

    #[test]
    fn allocation_pressure_probe() {
        if !is_probe("memory::tests::allocation_pressure_probe") {
            return;
        }
        let mut stack_top = 0_u8;
        init_main_thread(&mut stack_top);
        let survivor = std::hint::black_box(alloc_bytes(32));
        // SAFETY: survivor points to 32 writable bytes.
        unsafe { survivor.write(73) };

        for _ in 0..4096 {
            std::hint::black_box(alloc_bytes(256));
        }

        let (collections, live_bytes) = heap::stats();
        assert!(collections > 0, "automatic collection never ran");
        assert!(live_bytes < 4096 * 256, "live bytes {live_bytes}");
        // SAFETY: survivor stays rooted by this frame.
        assert_eq!(unsafe { survivor.read() }, 73);
    }
}
