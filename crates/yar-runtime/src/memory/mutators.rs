use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

#[unsafe(no_mangle)]
pub static yar_gc_safepoint_requested: AtomicU32 = AtomicU32::new(0);

static REGISTRY: Mutex<Registry> = Mutex::new(Registry::new());
static REGISTRY_CHANGED: Condvar = Condvar::new();

thread_local! {
    static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
}

type SpilledFn = extern "C" fn(*mut c_void, *mut c_void);

unsafe extern "C" {
    fn yar_gc_spill_registers_and_call(callback: SpilledFn, context: *mut c_void);
    fn yar_gc_thread_stack_top(marker: *mut c_void) -> *mut c_void;
}

#[derive(Clone, Copy)]
pub(super) struct StackRange {
    pub(super) low: usize,
    pub(super) high: usize,
}

struct Mutator {
    id: usize,
    stack_top: usize,
    stopped_at: Option<usize>,
}

struct Registry {
    mutators: Vec<Mutator>,
    next_id: usize,
    collecting: bool,
}

impl Registry {
    const fn new() -> Self {
        Self {
            mutators: Vec::new(),
            next_id: 1,
            collecting: false,
        }
    }

    fn mutator(&mut self, id: usize) -> &mut Mutator {
        self.mutators
            .iter_mut()
            .find(|mutator| mutator.id == id)
            .unwrap_or_else(|| {
                crate::runtime_fail(b"runtime failure: collector thread registry corrupted\n")
            })
    }

    fn all_stopped(&self) -> bool {
        self.mutators
            .iter()
            .all(|mutator| mutator.stopped_at.is_some())
    }
}

pub(crate) struct Registration {
    id: usize,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut registry = lock_registry();
        registry.mutators.retain(|mutator| mutator.id != self.id);
        CURRENT.set(None);
        REGISTRY_CHANGED.notify_all();
    }
}

pub(crate) fn register_current_thread(marker: *mut u8) -> Registration {
    if CURRENT.get().is_some() {
        crate::runtime_fail(b"runtime failure: collector thread registered twice\n");
    }
    // SAFETY: the shim only inspects the calling thread's stack limits.
    let stack_top = unsafe { yar_gc_thread_stack_top(marker.cast()) } as usize;
    let mut registry = lock_registry();
    while registry.collecting {
        registry = wait_registry(registry);
    }
    let id = registry.next_id;
    registry.next_id += 1;
    registry.mutators.push(Mutator {
        id,
        stack_top,
        stopped_at: None,
    });
    CURRENT.set(Some(id));
    Registration { id }
}

pub(crate) fn safepoint() {
    if yar_gc_safepoint_requested.load(Ordering::Acquire) == 0 {
        return;
    }
    let Some(id) = CURRENT.get() else {
        return;
    };
    with_spilled_registers(|stack_low| {
        stop(id, stack_low);
        resume(id);
    });
}

pub(crate) fn blocking<R>(operation: impl FnOnce() -> R) -> R {
    let Some(id) = CURRENT.get() else {
        return operation();
    };
    let mut result = None;
    with_spilled_registers(|stack_low| {
        stop(id, stack_low);
        result = Some(operation());
        resume(id);
    });
    result.unwrap_or_else(|| {
        crate::runtime_fail(b"runtime failure: collector blocking region lost\n")
    })
}

pub(crate) fn wait<'a, T>(
    condvar: &Condvar,
    mutex: &'a Mutex<T>,
    guard: MutexGuard<'a, T>,
) -> MutexGuard<'a, T> {
    let Some(id) = CURRENT.get() else {
        return condvar.wait(guard).unwrap_or_else(|err| err.into_inner());
    };
    let mut reacquired = None;
    with_spilled_registers(|stack_low| {
        stop(id, stack_low);
        let woken = condvar.wait(guard).unwrap_or_else(|err| err.into_inner());
        if try_resume(id) {
            reacquired = Some(woken);
            return;
        }
        drop(woken);
        resume(id);
    });
    reacquired.unwrap_or_else(|| mutex.lock().unwrap_or_else(|err| err.into_inner()))
}

pub(super) fn stop_the_world(collect: impl FnOnce(&[StackRange])) {
    let Some(id) = CURRENT.get() else {
        return;
    };
    let mut collect = Some(collect);
    with_spilled_registers(|stack_low| {
        let mut registry = lock_registry();
        if registry.collecting {
            drop(registry);
            stop(id, stack_low);
            resume(id);
            return;
        }
        registry.collecting = true;
        yar_gc_safepoint_requested.store(1, Ordering::Release);
        registry.mutator(id).stopped_at = Some(stack_low);
        while !registry.all_stopped() {
            registry = wait_registry(registry);
        }
        let stacks = registry
            .mutators
            .iter()
            .filter_map(|mutator| {
                mutator.stopped_at.map(|low| StackRange {
                    low,
                    high: mutator.stack_top,
                })
            })
            .collect::<Vec<_>>();
        if let Some(collect) = collect.take() {
            collect(&stacks);
        }
        registry.collecting = false;
        yar_gc_safepoint_requested.store(0, Ordering::Release);
        registry.mutator(id).stopped_at = None;
        REGISTRY_CHANGED.notify_all();
    });
}

fn stop(id: usize, stack_low: usize) {
    let mut registry = lock_registry();
    registry.mutator(id).stopped_at = Some(stack_low);
    REGISTRY_CHANGED.notify_all();
}

fn resume(id: usize) {
    let mut registry = lock_registry();
    while registry.collecting {
        registry = wait_registry(registry);
    }
    registry.mutator(id).stopped_at = None;
}

fn try_resume(id: usize) -> bool {
    let mut registry = lock_registry();
    if registry.collecting {
        return false;
    }
    registry.mutator(id).stopped_at = None;
    true
}

fn with_spilled_registers<F: FnOnce(usize)>(operation: F) {
    let mut operation = Some(operation);
    // SAFETY: the shim calls the trampoline synchronously on this thread
    // while `operation` is alive, passing its address back unchanged.
    unsafe {
        yar_gc_spill_registers_and_call(
            spilled_trampoline::<F>,
            (&mut operation as *mut Option<F>).cast(),
        );
    }
}

extern "C" fn spilled_trampoline<F: FnOnce(usize)>(context: *mut c_void, stack_low: *mut c_void) {
    // SAFETY: with_spilled_registers passes a live Option<F> for this call.
    let operation = unsafe { &mut *context.cast::<Option<F>>() };
    let marker = 0_usize;
    let marker_address = std::hint::black_box(&marker) as *const usize as usize;
    if let Some(operation) = operation.take() {
        operation((stack_low as usize).min(marker_address));
    }
}

fn lock_registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|err| err.into_inner())
}

fn wait_registry(registry: MutexGuard<'static, Registry>) -> MutexGuard<'static, Registry> {
    REGISTRY_CHANGED
        .wait(registry)
        .unwrap_or_else(|err| err.into_inner())
}
