use std::alloc::{self, Layout};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

use super::Descriptor;

const PAGE_SHIFT: u32 = 16;
pub(super) const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
const ADDRESS_BITS: u32 = 48;
const LEAF_BITS: u32 = 16;
const ROOT_BITS: u32 = ADDRESS_BITS - PAGE_SHIFT - LEAF_BITS;
const WORD: usize = size_of::<usize>();
const RETAINED_FREE_PAGES: usize = 64;
const PARALLEL_MARK_MIN_BYTES: usize = 8 * 1024 * 1024;
const MAX_MARK_WORKERS: usize = 8;
const MARK_SHARE_THRESHOLD: usize = 256;

const SIZE_CLASSES: [usize; 50] = [
    16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256, 288, 320, 352, 384,
    416, 448, 480, 512, 576, 640, 704, 768, 896, 1024, 1152, 1280, 1408, 1536, 1792, 2048, 2304,
    2560, 3072, 3584, 4096, 5120, 6144, 7168, 8192, 10240, 13056, 16384, 21760, 32768,
];
const MAX_SMALL_SIZE: usize = SIZE_CLASSES[SIZE_CLASSES.len() - 1];

type Leaf = [AtomicPtr<Page>; 1 << LEAF_BITS];

static PAGE_MAP: [AtomicPtr<Leaf>; 1 << ROOT_BITS] =
    [const { AtomicPtr::new(ptr::null_mut()) }; 1 << ROOT_BITS];
static STATE: Mutex<HeapState> = Mutex::new(HeapState::new());
static EPOCH: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_SINCE_COLLECTION: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);

thread_local! {
    static CACHE: RefCell<AllocationCache> = const { RefCell::new(AllocationCache::new()) };
}

struct Page {
    base: usize,
    object_size: usize,
    capacity: usize,
    span: usize,
    class: Option<usize>,
    live: Box<[AtomicU64]>,
    marks: Box<[AtomicU64]>,
    descriptors: Box<[AtomicPtr<Descriptor>]>,
    owned: AtomicBool,
}

impl Page {
    fn new(base: usize, object_size: usize, span: usize, class: Option<usize>) -> Self {
        let capacity = if class.is_some() {
            PAGE_SIZE / object_size
        } else {
            1
        };
        let words = capacity.div_ceil(64);
        Self {
            base,
            object_size,
            capacity,
            span,
            class,
            live: (0..words).map(|_| AtomicU64::new(0)).collect(),
            marks: (0..words).map(|_| AtomicU64::new(0)).collect(),
            descriptors: (0..capacity)
                .map(|_| AtomicPtr::new(ptr::null_mut()))
                .collect(),
            owned: AtomicBool::new(false),
        }
    }

    fn object_address(&self, slot: usize) -> usize {
        self.base + slot * self.object_size
    }

    fn slot_of(&self, address: usize) -> Option<usize> {
        let offset = address.checked_sub(self.base)?;
        let slot = offset / self.object_size;
        (slot < self.capacity).then_some(slot)
    }

    fn is_live(&self, slot: usize) -> bool {
        self.live[slot / 64].load(Ordering::Relaxed) & (1 << (slot % 64)) != 0
    }

    fn try_mark(&self, slot: usize) -> bool {
        let bit = 1 << (slot % 64);
        self.marks[slot / 64].fetch_or(bit, Ordering::Relaxed) & bit == 0
    }

    fn is_marked(&self, slot: usize) -> bool {
        self.marks[slot / 64].load(Ordering::Relaxed) & (1 << (slot % 64)) != 0
    }

    fn live_count(&self) -> usize {
        self.live
            .iter()
            .map(|word| word.load(Ordering::Relaxed).count_ones() as usize)
            .sum()
    }

    fn free_slots(&self) -> usize {
        self.capacity - self.live_count()
    }

    fn claim_free_slot(&self, from: usize) -> Option<usize> {
        let mut word_index = from / 64;
        let mut skip = from % 64;
        while word_index < self.live.len() {
            let live = self.live[word_index].load(Ordering::Relaxed);
            let free = !live & (u64::MAX << skip);
            if free != 0 {
                let slot = word_index * 64 + free.trailing_zeros() as usize;
                if slot >= self.capacity {
                    return None;
                }
                self.live[word_index].fetch_or(1 << (slot % 64), Ordering::Relaxed);
                return Some(slot);
            }
            word_index += 1;
            skip = 0;
        }
        None
    }

    fn initialize_slot(&self, slot: usize, descriptor: *const Descriptor) -> *mut u8 {
        self.descriptors[slot].store(descriptor.cast_mut(), Ordering::Relaxed);
        let object = self.object_address(slot) as *mut u8;
        // SAFETY: slot lies inside this page's owned memory and was just
        // claimed by the allocating thread, so no other thread writes it.
        unsafe { ptr::write_bytes(object, 0, self.object_size) };
        object
    }

    fn bytes(&self) -> usize {
        self.span * PAGE_SIZE
    }
}

struct HeapState {
    pages: Vec<NonNull<Page>>,
    partial: [Vec<NonNull<Page>>; SIZE_CLASSES.len()],
    free_pages: Vec<usize>,
    finalizers: BTreeMap<usize, fn(*mut u8)>,
    live_bytes: usize,
    collections: usize,
}

// SAFETY: page pointers are only dereferenced under the heap lock or while
// the world is stopped; Page itself only holds atomics and immutable fields.
unsafe impl Send for HeapState {}

impl HeapState {
    const fn new() -> Self {
        Self {
            pages: Vec::new(),
            partial: [const { Vec::new() }; SIZE_CLASSES.len()],
            free_pages: Vec::new(),
            finalizers: BTreeMap::new(),
            live_bytes: 0,
            collections: 0,
        }
    }

    fn acquire_page(&mut self, class: usize) -> Option<NonNull<Page>> {
        let page = match self.partial[class].pop() {
            Some(page) => page,
            None => self.new_page(class)?,
        };
        // SAFETY: registered pages stay allocated until the heap releases them
        // under this lock.
        let page_ref = unsafe { page.as_ref() };
        page_ref.owned.store(true, Ordering::Relaxed);
        ALLOCATED_SINCE_COLLECTION.fetch_add(
            page_ref.free_slots() * page_ref.object_size,
            Ordering::Relaxed,
        );
        Some(page)
    }

    fn new_page(&mut self, class: usize) -> Option<NonNull<Page>> {
        let base = match self.free_pages.pop() {
            Some(base) => base,
            None => allocate_span(1)?,
        };
        Some(self.register(Page::new(base, SIZE_CLASSES[class], 1, Some(class))))
    }

    fn allocate_large(&mut self, size: usize, descriptor: *const Descriptor) -> Option<*mut u8> {
        let span = size.div_ceil(PAGE_SIZE);
        let base = allocate_span(span)?;
        let page = self.register(Page::new(base, span * PAGE_SIZE, span, None));
        // SAFETY: the page was registered above and is not yet shared.
        let page = unsafe { page.as_ref() };
        page.live[0].store(1, Ordering::Relaxed);
        page.descriptors[0].store(descriptor.cast_mut(), Ordering::Relaxed);
        ALLOCATED_SINCE_COLLECTION.fetch_add(page.bytes(), Ordering::Relaxed);
        Some(base as *mut u8)
    }

    fn register(&mut self, page: Page) -> NonNull<Page> {
        let base = page.base;
        let span = page.span;
        let page = NonNull::from(Box::leak(Box::new(page)));
        for index in 0..span {
            page_map_slot(base + index * PAGE_SIZE).store(page.as_ptr(), Ordering::Release);
        }
        self.pages.push(page);
        page
    }

    fn release(&mut self, page: NonNull<Page>) {
        // SAFETY: the page is registered and no thread cache owns it.
        let page = unsafe { Box::from_raw(page.as_ptr()) };
        for index in 0..page.span {
            page_map_slot(page.base + index * PAGE_SIZE).store(ptr::null_mut(), Ordering::Release);
        }
        if page.span == 1 && self.free_pages.len() < RETAINED_FREE_PAGES {
            self.free_pages.push(page.base);
            return;
        }
        // SAFETY: base was returned by allocate_span with this span.
        unsafe { alloc::dealloc(page.base as *mut u8, span_layout(page.span)) };
    }

    fn return_owned_page(&mut self, page: NonNull<Page>) {
        // SAFETY: the calling thread cache owned this registered page.
        let page_ref = unsafe { page.as_ref() };
        page_ref.owned.store(false, Ordering::Relaxed);
        if let Some(class) = page_ref.class
            && page_ref.free_slots() > 0
        {
            self.partial[class].push(page);
        }
    }

    fn sweep(&mut self) {
        for class in &mut self.partial {
            class.clear();
        }
        let mut live_bytes = 0;
        let mut retained = Vec::with_capacity(self.pages.len());
        for page in std::mem::take(&mut self.pages) {
            // SAFETY: the world is stopped and every registered page is valid.
            let page_ref = unsafe { page.as_ref() };
            let mut live = 0;
            for (marks, live_word) in page_ref.marks.iter().zip(page_ref.live.iter()) {
                let word = marks.swap(0, Ordering::Relaxed);
                live_word.store(word, Ordering::Relaxed);
                live += word.count_ones() as usize;
            }
            let owned = page_ref.owned.load(Ordering::Relaxed);
            if live == 0 && !owned {
                self.release(page);
                continue;
            }
            live_bytes += live * page_ref.object_size;
            if let Some(class) = page_ref.class
                && !owned
                && live < page_ref.capacity
            {
                self.partial[class].push(page);
            }
            retained.push(page);
        }
        self.pages = retained;
        self.live_bytes = live_bytes;
        self.collections += 1;
    }
}

struct AllocationCache {
    epoch: usize,
    current: [Option<(NonNull<Page>, usize)>; SIZE_CLASSES.len()],
}

impl AllocationCache {
    const fn new() -> Self {
        Self {
            epoch: 0,
            current: [None; SIZE_CLASSES.len()],
        }
    }

    fn return_pages(&mut self, state: &mut HeapState) {
        for current in &mut self.current {
            if let Some((page, _)) = current.take() {
                state.return_owned_page(page);
            }
        }
    }
}

impl Drop for AllocationCache {
    fn drop(&mut self) {
        if self.current.iter().all(Option::is_none) {
            return;
        }
        self.return_pages(&mut lock_state());
    }
}

pub(super) fn allocate(size: usize, descriptor: *const Descriptor) -> Option<*mut u8> {
    let Some(class) = size_class(size) else {
        return lock_state().allocate_large(size, descriptor);
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let epoch = EPOCH.load(Ordering::Acquire);
        if cache.epoch != epoch {
            cache.return_pages(&mut lock_state());
            cache.epoch = epoch;
        }
        loop {
            if let Some((page, cursor)) = cache.current[class] {
                // SAFETY: this thread cache owns the page, which stays registered.
                let page_ref = unsafe { page.as_ref() };
                if let Some(slot) = page_ref.claim_free_slot(cursor) {
                    cache.current[class] = Some((page, slot + 1));
                    return Some(page_ref.initialize_slot(slot, descriptor));
                }
                cache.current[class] = None;
                lock_state().return_owned_page(page);
            }
            let page = lock_state().acquire_page(class)?;
            cache.current[class] = Some((page, 0));
        }
    })
}

pub(super) fn register_finalizer(object: *mut u8, finalizer: fn(*mut u8)) {
    lock_state().finalizers.insert(object as usize, finalizer);
}

pub(super) fn collection_due() -> bool {
    ALLOCATED_SINCE_COLLECTION.load(Ordering::Relaxed) >= ALLOCATION_BUDGET.load(Ordering::Relaxed)
}

pub(super) fn set_minimum_budget(bytes: usize) {
    ALLOCATION_BUDGET.store(bytes.max(1), Ordering::Relaxed);
}

pub(super) struct Collection<'a> {
    state: MutexGuard<'a, HeapState>,
    roots: Vec<usize>,
}

impl Collection<'_> {
    pub(super) fn begin() -> Self {
        Self {
            state: lock_state(),
            roots: Vec::new(),
        }
    }

    pub(super) fn add_root(&mut self, candidate: usize) {
        self.roots.push(candidate);
    }

    pub(super) fn add_conservative_range(&mut self, low: usize, high: usize) {
        let mut cursor = low.next_multiple_of(WORD);
        while cursor + WORD <= high {
            // SAFETY: the caller guarantees [low, high) is readable memory
            // that stays stable while the world is stopped.
            let candidate = unsafe { ptr::read(cursor as *const usize) };
            if candidate != 0 {
                self.roots.push(candidate);
            }
            cursor += WORD;
        }
    }

    pub(super) fn finish(mut self, minimum_budget: usize) {
        let parallel = self.state.live_bytes >= PARALLEL_MARK_MIN_BYTES;
        mark_from(std::mem::take(&mut self.roots), parallel);
        self.run_finalizers();
        self.state.sweep();
        let budget = self.state.live_bytes.max(minimum_budget);
        ALLOCATION_BUDGET.store(budget, Ordering::Relaxed);
        ALLOCATED_SINCE_COLLECTION.store(0, Ordering::Relaxed);
        EPOCH.fetch_add(1, Ordering::Release);
    }

    fn run_finalizers(&mut self) {
        let unreachable = self
            .state
            .finalizers
            .keys()
            .copied()
            .filter(|&object| !is_marked(object))
            .collect::<Vec<_>>();
        for object in unreachable {
            if let Some(finalizer) = self.state.finalizers.remove(&object) {
                finalizer(object as *mut u8);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn stats() -> (usize, usize) {
    let state = lock_state();
    (state.collections, state.live_bytes)
}

#[cfg(test)]
pub(super) fn is_live(address: usize) -> bool {
    find(address).is_some_and(|(page, slot)| page.is_live(slot))
}

fn lock_state() -> MutexGuard<'static, HeapState> {
    STATE.lock().unwrap_or_else(|err| err.into_inner())
}

fn size_class(size: usize) -> Option<usize> {
    if size > MAX_SMALL_SIZE {
        return None;
    }
    Some(SIZE_CLASSES.partition_point(|&class| class < size.max(1)))
}

fn span_layout(span: usize) -> Layout {
    Layout::from_size_align(span * PAGE_SIZE, PAGE_SIZE)
        .unwrap_or_else(|_| crate::runtime_fail(b"runtime failure: invalid allocation size\n"))
}

fn allocate_span(span: usize) -> Option<usize> {
    // SAFETY: span_layout is non-zero sized with a power-of-two alignment.
    let base = unsafe { alloc::alloc_zeroed(span_layout(span)) };
    if base.is_null() || (base as usize) >> ADDRESS_BITS != 0 {
        return None;
    }
    Some(base as usize)
}

fn page_map_slot(address: usize) -> &'static AtomicPtr<Page> {
    let root = &PAGE_MAP[address >> (PAGE_SHIFT + LEAF_BITS)];
    let mut leaf = root.load(Ordering::Acquire);
    if leaf.is_null() {
        let fresh: Box<Leaf> =
            Box::new([const { AtomicPtr::new(ptr::null_mut()) }; 1 << LEAF_BITS]);
        let fresh = Box::into_raw(fresh);
        match root.compare_exchange(ptr::null_mut(), fresh, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => leaf = fresh,
            Err(existing) => {
                // SAFETY: fresh lost the race and was never published.
                drop(unsafe { Box::from_raw(fresh) });
                leaf = existing;
            }
        }
    }
    // SAFETY: published leaves are never freed.
    let leaf = unsafe { &*leaf };
    &leaf[(address >> PAGE_SHIFT) & ((1 << LEAF_BITS) - 1)]
}

fn find(address: usize) -> Option<(&'static Page, usize)> {
    if address >> ADDRESS_BITS != 0 {
        return None;
    }
    let leaf = PAGE_MAP[address >> (PAGE_SHIFT + LEAF_BITS)].load(Ordering::Acquire);
    if leaf.is_null() {
        return None;
    }
    // SAFETY: published leaves are never freed.
    let page =
        unsafe { &*leaf }[(address >> PAGE_SHIFT) & ((1 << LEAF_BITS) - 1)].load(Ordering::Acquire);
    if page.is_null() {
        return None;
    }
    // SAFETY: mapped pages stay allocated until release unmaps them, which
    // only happens under the heap lock held by the collector.
    let page = unsafe { &*page };
    let slot = page.slot_of(address)?;
    Some((page, slot))
}

fn is_marked(address: usize) -> bool {
    find(address).is_some_and(|(page, slot)| page.is_marked(slot))
}

fn mark_candidate(candidate: usize, stack: &mut Vec<usize>) {
    let Some((page, slot)) = find(candidate) else {
        return;
    };
    if page.is_live(slot) && page.try_mark(slot) {
        stack.push(page.object_address(slot));
    }
}

fn trace_object(object: usize, stack: &mut Vec<usize>) {
    let Some((page, slot)) = find(object) else {
        return;
    };
    let descriptor = page.descriptors[slot].load(Ordering::Relaxed);
    if descriptor.is_null() {
        return;
    }
    // SAFETY: descriptors are complete static data emitted by codegen or the
    // runtime.
    let (stride, offsets) = unsafe { Descriptor::layout(descriptor) };
    let end = object + page.object_size;
    let mut element = object;
    while element + stride <= end {
        for &offset in offsets {
            // SAFETY: element + offset + WORD <= element + stride <= end, which
            // lies inside the live object.
            let candidate = unsafe { ptr::read((element + offset as usize) as *const usize) };
            if candidate != 0 {
                mark_candidate(candidate, stack);
            }
        }
        element += stride;
    }
}

fn mark_from(roots: Vec<usize>, parallel: bool) {
    let mut stack = Vec::new();
    for root in roots {
        mark_candidate(root, &mut stack);
    }
    if !parallel {
        drain(&mut stack, None);
        return;
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_MARK_WORKERS);
    let shared = MarkQueue::new(workers);
    shared.push(stack);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut local = Vec::new();
                while shared.take(&mut local) {
                    drain(&mut local, Some(&shared));
                }
            });
        }
    });
}

fn drain(stack: &mut Vec<usize>, shared: Option<&MarkQueue>) {
    while let Some(object) = stack.pop() {
        trace_object(object, stack);
        if let Some(shared) = shared
            && stack.len() > MARK_SHARE_THRESHOLD
        {
            let half = stack.split_off(stack.len() / 2);
            shared.push(half);
        }
    }
}

struct MarkQueue {
    state: Mutex<MarkQueueState>,
    available: Condvar,
}

struct MarkQueueState {
    batches: Vec<Vec<usize>>,
    idle: usize,
    workers: usize,
    done: bool,
}

impl MarkQueue {
    fn new(workers: usize) -> Self {
        Self {
            state: Mutex::new(MarkQueueState {
                batches: Vec::new(),
                idle: 0,
                workers,
                done: false,
            }),
            available: Condvar::new(),
        }
    }

    fn push(&self, batch: Vec<usize>) {
        if batch.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.batches.push(batch);
        self.available.notify_one();
    }

    fn take(&self, local: &mut Vec<usize>) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        loop {
            if let Some(batch) = state.batches.pop() {
                *local = batch;
                return true;
            }
            if state.done {
                return false;
            }
            state.idle += 1;
            if state.idle == state.workers {
                state.done = true;
                self.available.notify_all();
                return false;
            }
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|err| err.into_inner());
            state.idle -= 1;
        }
    }
}
