use std::alloc::{self, Layout};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const HEADER_SIZE: usize = mem::size_of::<usize>();
const ALIGN: usize = mem::align_of::<usize>();
const DEFAULT_HEAP_TARGET: usize = 1024 * 1024;

thread_local! {
    static STACK_TOP: Cell<usize> = const { Cell::new(0) };
    static COLLECTION_INHIBIT: Cell<usize> = const { Cell::new(0) };
}

static HEAP: OnceLock<Mutex<Heap>> = OnceLock::new();
static COLLECTING: AtomicBool = AtomicBool::new(false);

type RootVisitor = extern "C" fn(usize, *mut c_void);

unsafe extern "C" {
    fn yar_gc_visit_stack_and_registers(
        stack_top: *const u8,
        visitor: RootVisitor,
        context: *mut c_void,
    );
}

#[derive(Clone, Copy)]
struct AllocationLayout {
    layout: Layout,
    logical_size: usize,
    physical_size: usize,
}

struct Block {
    base: usize,
    physical_size: usize,
    layout: Layout,
    marked: bool,
}

struct Heap {
    blocks: BTreeMap<usize, Block>,
    live_bytes: usize,
    target: usize,
    minimum_target: usize,
    collections: usize,
}

impl Heap {
    fn new(target: usize) -> Self {
        let target = target.max(1);
        Self {
            blocks: BTreeMap::new(),
            live_bytes: 0,
            target,
            minimum_target: target,
            collections: 0,
        }
    }

    fn allocate(&mut self, allocation: AllocationLayout) -> Option<*mut u8> {
        // Managed bytes are always initialized before the block enters the heap
        // registry. Conservative traversal may inspect padding or a partially
        // populated aggregate during a later nested allocation.
        let base = unsafe { alloc::alloc_zeroed(allocation.layout) };
        if base.is_null() {
            return None;
        }

        // SAFETY: allocation.layout reserves HEADER_SIZE followed by at least
        // one payload byte, and base is aligned for usize.
        let payload = unsafe {
            ptr::write(base.cast::<usize>(), allocation.logical_size);
            base.add(HEADER_SIZE)
        };
        let payload_address = payload as usize;
        let old = self.blocks.insert(
            payload_address,
            Block {
                base: base as usize,
                physical_size: allocation.physical_size,
                layout: allocation.layout,
                marked: false,
            },
        );
        if old.is_some() {
            super::runtime_fail(b"runtime failure: collector metadata corrupted\n");
        }
        self.live_bytes = self
            .live_bytes
            .checked_add(allocation.physical_size)
            .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid allocation size\n"));
        self.grow_target_for_allocation();
        Some(payload)
    }

    fn grow_target_for_allocation(&mut self) {
        if self.live_bytes <= self.target {
            return;
        }
        self.target = self.live_bytes.saturating_mul(2).max(self.minimum_target);
    }

    fn mark_candidate(&mut self, candidate: usize, queue: &mut Vec<usize>) {
        let Some((&payload, block)) = self.blocks.range_mut(..=candidate).next_back() else {
            return;
        };
        let Some(end) = payload.checked_add(block.physical_size) else {
            return;
        };
        if candidate >= end || block.marked {
            return;
        }
        block.marked = true;
        queue.push(payload);
    }

    fn mark_bytes(&mut self, bytes: &[u8], queue: &mut Vec<usize>) {
        let word_size = mem::size_of::<usize>();
        if bytes.len() < word_size {
            return;
        }
        for offset in 0..=bytes.len() - word_size {
            let mut word = [0_u8; mem::size_of::<usize>()];
            word.copy_from_slice(&bytes[offset..offset + word_size]);
            let candidate = usize::from_ne_bytes(word);
            if candidate != 0 {
                self.mark_candidate(candidate, queue);
            }
        }
    }

    fn trace(&mut self, queue: &mut Vec<usize>) {
        while let Some(payload) = queue.pop() {
            let Some(physical_size) = self.blocks.get(&payload).map(|block| block.physical_size)
            else {
                continue;
            };
            // SAFETY: every managed block is zero-initialized before registry
            // insertion and remains allocated until sweep, which starts only
            // after traversal completes.
            let bytes = unsafe { std::slice::from_raw_parts(payload as *const u8, physical_size) };
            self.mark_bytes(bytes, queue);
        }
    }

    fn sweep(&mut self) {
        let unreachable = self
            .blocks
            .iter()
            .filter_map(|(&payload, block)| (!block.marked).then_some(payload))
            .collect::<Vec<_>>();
        for payload in unreachable {
            let block = self.blocks.remove(&payload).unwrap_or_else(|| {
                super::runtime_fail(b"runtime failure: collector metadata corrupted\n")
            });
            self.live_bytes -= block.physical_size;
            // SAFETY: base and layout are the exact pair returned by alloc_zeroed
            // for this block, and the block has been removed from the registry.
            unsafe { alloc::dealloc(block.base as *mut u8, block.layout) };
        }
        for block in self.blocks.values_mut() {
            block.marked = false;
        }
        self.collections = self.collections.saturating_add(1);
        self.target = self.live_bytes.saturating_mul(2).max(self.minimum_target);
    }

    #[cfg(test)]
    fn collect_candidates(&mut self, candidates: &[usize]) {
        let mut queue = Vec::new();
        for &candidate in candidates {
            self.mark_candidate(candidate, &mut queue);
        }
        self.trace(&mut queue);
        self.sweep();
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        for block in self.blocks.values() {
            // SAFETY: test-owned heaps drop each still-registered allocation
            // exactly once with its original layout.
            unsafe { alloc::dealloc(block.base as *mut u8, block.layout) };
        }
    }
}

struct Marker<'a> {
    heap: &'a mut Heap,
    queue: Vec<usize>,
}

pub(crate) struct CollectionGuard;

impl Drop for CollectionGuard {
    fn drop(&mut self) {
        let depth = COLLECTION_INHIBIT.get();
        if depth == 0 {
            super::runtime_fail(b"runtime failure: collector guard corrupted\n");
        }
        COLLECTION_INHIBIT.set(depth - 1);
    }
}

impl Marker<'_> {
    fn visit(&mut self, candidate: usize) {
        self.heap.mark_candidate(candidate, &mut self.queue);
    }
}

extern "C" fn visit_root(candidate: usize, context: *mut c_void) {
    if context.is_null() {
        return;
    }
    // SAFETY: collect passes a live Marker pointer to the synchronous C root
    // visitor and does not access the marker until the visitor returns.
    let marker = unsafe { &mut *context.cast::<Marker<'_>>() };
    marker.visit(candidate);
}

pub(crate) fn init_stack_top(stack_top: *mut u8) {
    STACK_TOP.set(stack_top as usize);
}

pub(crate) fn inhibit_collection() -> CollectionGuard {
    let depth = COLLECTION_INHIBIT
        .get()
        .checked_add(1)
        .unwrap_or_else(|| super::runtime_fail(b"runtime failure: collector guard corrupted\n"));
    COLLECTION_INHIBIT.set(depth);
    CollectionGuard
}

pub(crate) fn alloc(size: i64, _zeroed: bool) -> *mut u8 {
    let allocation = allocation_layout(size);
    maybe_collect(allocation.physical_size);
    if let Some(payload) = heap()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .allocate(allocation)
    {
        return payload;
    }

    collect();
    heap()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .allocate(allocation)
        .unwrap_or_else(|| super::yar_trap_oom())
}

pub(crate) fn collect() {
    if super::concurrency::unjoined_tasks() != 0 || COLLECTION_INHIBIT.get() != 0 {
        return;
    }
    let stack_top = STACK_TOP.get();
    if stack_top == 0
        || COLLECTING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        return;
    }

    let external_roots = super::concurrency::channel_root_snapshots();
    let mut heap = heap().lock().unwrap_or_else(|err| err.into_inner());
    let mut marker = Marker {
        heap: &mut heap,
        queue: Vec::new(),
    };

    // SAFETY: stack_top was registered by the generated main wrapper on this
    // thread. The C shim captures ABI-preserved registers and visits the live
    // byte range between its current frame and that outer marker synchronously.
    unsafe {
        yar_gc_visit_stack_and_registers(
            stack_top as *const u8,
            visit_root,
            (&mut marker as *mut Marker<'_>).cast::<c_void>(),
        );
    }
    for roots in &external_roots {
        marker.heap.mark_bytes(roots, &mut marker.queue);
    }
    marker.heap.trace(&mut marker.queue);
    marker.heap.sweep();
    COLLECTING.store(false, Ordering::SeqCst);
}

fn maybe_collect(incoming_size: usize) {
    if super::concurrency::unjoined_tasks() != 0
        || STACK_TOP.get() == 0
        || COLLECTION_INHIBIT.get() != 0
    {
        return;
    }
    let should_collect = {
        let heap = heap().lock().unwrap_or_else(|err| err.into_inner());
        heap.live_bytes
            .checked_add(incoming_size)
            .is_none_or(|required| required > heap.target)
    };
    if should_collect {
        collect();
    }
}

fn heap() -> &'static Mutex<Heap> {
    HEAP.get_or_init(|| Mutex::new(Heap::new(configured_heap_target())))
}

fn configured_heap_target() -> usize {
    std::env::var("YAR_GC_HEAP_TARGET_BYTES")
        .ok()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(DEFAULT_HEAP_TARGET)
}

fn allocation_layout(size: i64) -> AllocationLayout {
    if size < 0 {
        super::runtime_fail(b"runtime failure: invalid allocation size\n");
    }
    let logical_size = usize::try_from(size)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid allocation size\n"));
    let physical_size = logical_size.max(1);
    let total_size = HEADER_SIZE
        .checked_add(physical_size)
        .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid allocation size\n"));
    let layout = Layout::from_size_align(total_size, ALIGN)
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid allocation size\n"));
    AllocationLayout {
        layout,
        logical_size,
        physical_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn retains_interior_and_unaligned_transitive_roots_then_reclaims_them() {
        let mut heap = Heap::new(64);
        let parent = heap.allocate(allocation_layout(32)).unwrap();
        let child = heap.allocate(allocation_layout(8)).unwrap();
        // Store the child pointer at offset one to model packed map buckets.
        unsafe {
            ptr::copy_nonoverlapping(
                (child as usize).to_ne_bytes().as_ptr(),
                parent.add(1),
                mem::size_of::<usize>(),
            );
        }

        heap.collect_candidates(&[parent as usize + 7]);
        assert_eq!(heap.blocks.len(), 2);

        heap.collect_candidates(&[]);
        assert!(heap.blocks.is_empty());
        assert_eq!(heap.live_bytes, 0);
        assert_eq!(heap.collections, 2);
    }

    #[test]
    fn rejects_header_and_one_past_end_as_roots() {
        let mut heap = Heap::new(64);
        let payload = heap.allocate(allocation_layout(8)).unwrap();
        heap.collect_candidates(&[payload as usize - 1, payload as usize + 8]);
        assert!(heap.blocks.is_empty());
    }

    #[test]
    fn zero_sized_allocations_have_one_retainable_payload_byte() {
        let mut heap = Heap::new(1);
        let payload = heap.allocate(allocation_layout(0)).unwrap();
        heap.collect_candidates(&[payload as usize]);
        assert_eq!(heap.blocks.len(), 1);
        assert_eq!(heap.live_bytes, 1);
        heap.collect_candidates(&[]);
        assert!(heap.blocks.is_empty());
    }

    #[test]
    fn automatic_collection_reclaims_in_an_isolated_runtime() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "memory::tests::automatic_collection_probe",
                "--nocapture",
            ])
            .env("YAR_GC_TEST_PROBE", "1")
            .env("YAR_GC_HEAP_TARGET_BYTES", "1024")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn automatic_collection_probe() {
        if std::env::var_os("YAR_GC_TEST_PROBE").is_none() {
            return;
        }

        let mut stack_top = 0_u8;
        init_stack_top(&mut stack_top);
        let survivor = super::super::yar_alloc_zeroed(32);
        unsafe { survivor.write(73) };

        const ALLOCATIONS: usize = 512;
        const ALLOCATION_SIZE: usize = 256;
        for value in 0..ALLOCATIONS {
            let garbage = super::super::yar_alloc(ALLOCATION_SIZE as i64);
            unsafe { garbage.write(value as u8) };
            std::hint::black_box(garbage);
        }

        assert_eq!(unsafe { survivor.read() }, 73);
        let heap = heap().lock().unwrap_or_else(|err| err.into_inner());
        assert!(heap.collections > 0, "automatic collection never ran");
        assert!(
            heap.live_bytes < ALLOCATIONS * ALLOCATION_SIZE,
            "automatic collection did not reduce live bytes: {}",
            heap.live_bytes
        );
    }
}
