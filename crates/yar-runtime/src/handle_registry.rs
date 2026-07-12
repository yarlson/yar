use std::{
    fs::File,
    net::{TcpListener, TcpStream},
    ops::Deref,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub(crate) type FileHandle = Arc<Mutex<Option<File>>>;
pub(crate) type StringBuilderHandle = Arc<Mutex<Vec<u8>>>;

pub(crate) struct ListenerLease(Arc<ListenerState>);

impl Deref for ListenerLease {
    type Target = ListenerState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ListenerLease {
    fn drop(&mut self) {
        self.0.operations.finish();
    }
}

pub(crate) struct ConnectionLease(Arc<ConnectionState>);

impl Deref for ConnectionLease {
    type Target = ConnectionState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        self.0.operations.finish();
    }
}

struct Operations {
    active: Mutex<usize>,
    idle: Condvar,
}

impl Operations {
    fn new() -> Self {
        Self {
            active: Mutex::new(0),
            idle: Condvar::new(),
        }
    }

    fn start(&self) {
        let mut active = self.active.lock().unwrap_or_else(|err| err.into_inner());
        *active = active.checked_add(1).unwrap_or_else(|| {
            crate::runtime_fail(b"runtime failure: resource operation count exhausted\n")
        });
    }

    fn finish(&self) {
        let mut active = self.active.lock().unwrap_or_else(|err| err.into_inner());
        *active = active.checked_sub(1).unwrap_or_else(|| {
            crate::runtime_fail(b"runtime failure: inconsistent resource operation count\n")
        });
        if *active == 0 {
            self.idle.notify_all();
        }
    }

    fn wait(&self) {
        let mut active = self.active.lock().unwrap_or_else(|err| err.into_inner());
        while *active != 0 {
            active = self
                .idle
                .wait(active)
                .unwrap_or_else(|err| err.into_inner());
        }
    }
}

pub(crate) struct ListenerState {
    pub(crate) listener: TcpListener,
    pub(crate) closed: AtomicBool,
    operations: Operations,
    pub(crate) accept: Mutex<()>,
}

impl ListenerState {
    pub(crate) fn wait_for_operations(&self) {
        self.operations.wait();
    }
}

pub(crate) struct ConnectionState {
    pub(crate) stream: TcpStream,
    pub(crate) closed: AtomicBool,
    operations: Operations,
    pub(crate) read_timeout_millis: AtomicU64,
    pub(crate) write_timeout_millis: AtomicU64,
    pub(crate) read: Mutex<()>,
    pub(crate) write: Mutex<()>,
}

impl ConnectionState {
    pub(crate) fn wait_for_operations(&self) {
        self.operations.wait();
    }
}

#[derive(Clone)]
enum HandleEntry {
    File(FileHandle),
    Listener(Arc<ListenerState>),
    Connection(Arc<ConnectionState>),
    StringBuilder(StringBuilderHandle),
}

impl HandleEntry {
    fn kind(&self) -> HandleKind {
        match self {
            Self::File(_) => HandleKind::File,
            Self::Listener(_) => HandleKind::Listener,
            Self::Connection(_) => HandleKind::Connection,
            Self::StringBuilder(_) => HandleKind::StringBuilder,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum HandleKind {
    File,
    Listener,
    Connection,
    StringBuilder,
}

const SLOT_BITS: u32 = 31;
const MAX_SLOT_NUMBER: u32 = (1 << SLOT_BITS) - 1;
const HANDLE_MARKER: u64 = 1 << 62;
const MAX_GENERATION: u32 = (1 << 31) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RawHandle {
    generation: u32,
    slot: u32,
}

impl RawHandle {
    fn encode(self) -> i64 {
        debug_assert!(self.generation != 0);
        debug_assert!(self.generation <= MAX_GENERATION);
        debug_assert!(self.slot < MAX_SLOT_NUMBER);
        let slot_number = u64::from(self.slot) + 1;
        let raw = HANDLE_MARKER | (u64::from(self.generation) << SLOT_BITS) | slot_number;
        debug_assert!(raw <= i64::MAX as u64);
        raw as i64
    }

    fn decode(raw: i64) -> Option<Self> {
        let raw = u64::try_from(raw).ok()?;
        if raw & HANDLE_MARKER == 0 {
            return None;
        }
        let slot_number = (raw & u64::from(MAX_SLOT_NUMBER)) as u32;
        let generation = ((raw >> SLOT_BITS) & u64::from(MAX_GENERATION)) as u32;
        if slot_number == 0 || generation == 0 {
            return None;
        }
        Some(Self {
            generation,
            slot: slot_number - 1,
        })
    }
}

struct Slot {
    generation: u32,
    entry: Option<HandleEntry>,
    next_free: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InsertError {
    Exhausted,
    AllocationFailed,
    Corrupted,
}

struct HandleRegistry {
    slots: Vec<Slot>,
    free_head: Option<u32>,
    slot_limit: u32,
}

impl HandleRegistry {
    fn new() -> Self {
        Self::with_slot_limit(MAX_SLOT_NUMBER)
    }

    fn with_slot_limit(slot_limit: u32) -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            slot_limit: slot_limit.min(MAX_SLOT_NUMBER),
        }
    }

    fn insert(&mut self, entry: HandleEntry) -> i64 {
        match self.try_insert(entry) {
            Ok(id) => id,
            Err(InsertError::Exhausted) => {
                crate::runtime_fail(b"runtime failure: handle registry exhausted\n")
            }
            Err(InsertError::AllocationFailed) => {
                crate::runtime_fail(b"runtime failure: handle registry allocation failed\n")
            }
            Err(InsertError::Corrupted) => {
                crate::runtime_fail(b"runtime failure: handle registry corrupted\n")
            }
        }
    }

    fn try_insert(&mut self, entry: HandleEntry) -> Result<i64, InsertError> {
        if let Some(slot_index) = self.free_head {
            let slot = self
                .slots
                .get_mut(slot_index as usize)
                .ok_or(InsertError::Corrupted)?;
            if slot.entry.is_some() {
                return Err(InsertError::Corrupted);
            }
            self.free_head = slot.next_free.take();
            slot.entry = Some(entry);
            return Ok(RawHandle {
                generation: slot.generation,
                slot: slot_index,
            }
            .encode());
        }

        if self.slots.len() >= self.slot_limit as usize {
            return Err(InsertError::Exhausted);
        }
        self.slots
            .try_reserve(1)
            .map_err(|_| InsertError::AllocationFailed)?;
        let slot_index = u32::try_from(self.slots.len()).map_err(|_| InsertError::Exhausted)?;
        self.slots.push(Slot {
            generation: 1,
            entry: Some(entry),
            next_free: None,
        });
        Ok(RawHandle {
            generation: 1,
            slot: slot_index,
        }
        .encode())
    }

    fn get(&self, id: i64, kind: HandleKind) -> Option<HandleEntry> {
        let raw = RawHandle::decode(id)?;
        let slot = self.slots.get(raw.slot as usize)?;
        if slot.generation != raw.generation {
            return None;
        }
        slot.entry
            .as_ref()
            .filter(|entry| entry.kind() == kind)
            .cloned()
    }

    fn remove(&mut self, id: i64, kind: HandleKind) -> Option<HandleEntry> {
        let raw = RawHandle::decode(id)?;
        let slot = self.slots.get_mut(raw.slot as usize)?;
        if slot.generation != raw.generation
            || slot.entry.as_ref().is_none_or(|entry| entry.kind() != kind)
        {
            return None;
        }

        if let Some(entry) = slot.entry.as_ref() {
            match entry {
                HandleEntry::Listener(handle) => handle.closed.store(true, Ordering::Release),
                HandleEntry::Connection(handle) => handle.closed.store(true, Ordering::Release),
                HandleEntry::File(_) | HandleEntry::StringBuilder(_) => {}
            }
        }
        let entry = slot.entry.take();
        if slot.generation != MAX_GENERATION {
            slot.generation += 1;
            slot.next_free = self.free_head;
            self.free_head = Some(raw.slot);
        }
        entry
    }
}

static HANDLES: OnceLock<Mutex<HandleRegistry>> = OnceLock::new();

fn handles() -> MutexGuard<'static, HandleRegistry> {
    HANDLES
        .get_or_init(|| Mutex::new(HandleRegistry::new()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

pub(crate) fn register_file(file: File) -> i64 {
    handles().insert(HandleEntry::File(Arc::new(Mutex::new(Some(file)))))
}

pub(crate) fn file(id: i64) -> Option<FileHandle> {
    match handles().get(id, HandleKind::File)? {
        HandleEntry::File(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn remove_file(id: i64) -> Option<FileHandle> {
    match handles().remove(id, HandleKind::File)? {
        HandleEntry::File(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn register_listener(listener: TcpListener) -> i64 {
    handles().insert(HandleEntry::Listener(Arc::new(ListenerState {
        listener,
        closed: AtomicBool::new(false),
        operations: Operations::new(),
        accept: Mutex::new(()),
    })))
}

pub(crate) fn listener(id: i64) -> Option<ListenerLease> {
    let registry = handles();
    match registry.get(id, HandleKind::Listener)? {
        HandleEntry::Listener(handle) => {
            handle.operations.start();
            Some(ListenerLease(handle))
        }
        _ => None,
    }
}

pub(crate) fn remove_listener(id: i64) -> Option<Arc<ListenerState>> {
    let mut registry = handles();
    match registry.remove(id, HandleKind::Listener)? {
        HandleEntry::Listener(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn register_connection(stream: TcpStream) -> i64 {
    handles().insert(HandleEntry::Connection(Arc::new(ConnectionState {
        stream,
        closed: AtomicBool::new(false),
        operations: Operations::new(),
        read_timeout_millis: AtomicU64::new(0),
        write_timeout_millis: AtomicU64::new(0),
        read: Mutex::new(()),
        write: Mutex::new(()),
    })))
}

pub(crate) fn connection(id: i64) -> Option<ConnectionLease> {
    let registry = handles();
    match registry.get(id, HandleKind::Connection)? {
        HandleEntry::Connection(handle) => {
            handle.operations.start();
            Some(ConnectionLease(handle))
        }
        _ => None,
    }
}

pub(crate) fn remove_connection(id: i64) -> Option<Arc<ConnectionState>> {
    let mut registry = handles();
    match registry.remove(id, HandleKind::Connection)? {
        HandleEntry::Connection(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn register_string_builder() -> i64 {
    handles().insert(HandleEntry::StringBuilder(Arc::new(Mutex::new(
        Vec::with_capacity(64),
    ))))
}

pub(crate) fn string_builder(id: i64) -> Option<StringBuilderHandle> {
    match handles().get(id, HandleKind::StringBuilder)? {
        HandleEntry::StringBuilder(handle) => Some(handle),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder_entry() -> HandleEntry {
        HandleEntry::StringBuilder(Arc::new(Mutex::new(Vec::new())))
    }

    fn file_entry() -> HandleEntry {
        HandleEntry::File(Arc::new(Mutex::new(Some(
            File::open(std::env::current_exe().unwrap()).unwrap(),
        ))))
    }

    fn listener_entry() -> HandleEntry {
        HandleEntry::Listener(Arc::new(ListenerState {
            listener: TcpListener::bind("127.0.0.1:0").unwrap(),
            closed: AtomicBool::new(false),
            operations: Operations::new(),
            accept: Mutex::new(()),
        }))
    }

    #[test]
    fn raw_handles_round_trip_and_reject_invalid_shapes() {
        for raw in [
            RawHandle {
                generation: 1,
                slot: 0,
            },
            RawHandle {
                generation: MAX_GENERATION,
                slot: MAX_SLOT_NUMBER - 1,
            },
        ] {
            let encoded = raw.encode();
            assert!(encoded > 0);
            assert_eq!(RawHandle::decode(encoded), Some(raw));
        }
        assert_eq!(RawHandle::decode(0), None);
        assert_eq!(RawHandle::decode(-1), None);
        assert_eq!(RawHandle::decode(1), None);
        assert_eq!(RawHandle::decode((1_i64 << 31) | 1), None);
    }

    #[test]
    fn reused_slot_rejects_stale_and_wrong_kind_handles_without_consuming() {
        let mut registry = HandleRegistry::with_slot_limit(1);
        let stale = registry.try_insert(builder_entry()).unwrap();
        assert!(registry.remove(stale, HandleKind::File).is_none());
        assert!(registry.get(stale, HandleKind::StringBuilder).is_some());
        assert!(registry.remove(stale, HandleKind::StringBuilder).is_some());

        let current = registry.try_insert(file_entry()).unwrap();
        assert_ne!(stale, current);
        assert_eq!(RawHandle::decode(stale).unwrap().slot, 0);
        assert_eq!(RawHandle::decode(current).unwrap().slot, 0);
        assert!(registry.get(stale, HandleKind::StringBuilder).is_none());
        assert!(registry.remove(stale, HandleKind::File).is_none());
        assert!(registry.get(current, HandleKind::StringBuilder).is_none());
        assert!(registry.get(current, HandleKind::File).is_some());
    }

    #[test]
    fn maximum_generation_is_allocated_once_then_retired() {
        let mut registry = HandleRegistry::with_slot_limit(2);
        registry.slots.push(Slot {
            generation: MAX_GENERATION - 1,
            entry: None,
            next_free: None,
        });
        registry.free_head = Some(0);

        let penultimate = registry.try_insert(builder_entry()).unwrap();
        assert_eq!(
            RawHandle::decode(penultimate).unwrap().generation,
            MAX_GENERATION - 1
        );
        registry
            .remove(penultimate, HandleKind::StringBuilder)
            .unwrap();
        let final_handle = registry.try_insert(builder_entry()).unwrap();
        assert_eq!(
            RawHandle::decode(final_handle).unwrap().generation,
            MAX_GENERATION
        );
        registry
            .remove(final_handle, HandleKind::StringBuilder)
            .unwrap();

        let next = registry.try_insert(builder_entry()).unwrap();
        assert_eq!(RawHandle::decode(next).unwrap().slot, 1);
        assert!(
            registry
                .get(final_handle, HandleKind::StringBuilder)
                .is_none()
        );
    }

    #[test]
    fn slot_exhaustion_and_free_list_reuse_are_deterministic() {
        let mut registry = HandleRegistry::with_slot_limit(2);
        let first = registry.try_insert(builder_entry()).unwrap();
        let second = registry.try_insert(builder_entry()).unwrap();
        assert_eq!(
            registry.try_insert(builder_entry()),
            Err(InsertError::Exhausted)
        );

        registry.remove(first, HandleKind::StringBuilder).unwrap();
        registry.remove(second, HandleKind::StringBuilder).unwrap();
        let reused_second = registry.try_insert(builder_entry()).unwrap();
        let reused_first = registry.try_insert(builder_entry()).unwrap();
        assert_eq!(RawHandle::decode(reused_second).unwrap().slot, 1);
        assert_eq!(RawHandle::decode(reused_first).unwrap().slot, 0);
        assert_ne!(reused_first, reused_second);
    }

    #[test]
    fn string_builder_lookup_does_not_consume_or_recycle_the_handle() {
        let mut registry = HandleRegistry::with_slot_limit(1);
        let handle = registry.try_insert(builder_entry()).unwrap();
        assert!(registry.get(handle, HandleKind::StringBuilder).is_some());
        assert!(registry.get(handle, HandleKind::StringBuilder).is_some());
        assert_eq!(registry.free_head, None);
        assert_eq!(registry.slots.len(), 1);
    }

    #[test]
    fn listener_slot_reuse_keeps_old_lease_state_isolated() {
        let mut registry = HandleRegistry::with_slot_limit(1);
        let old_handle = registry.try_insert(listener_entry()).unwrap();
        let old_state = match registry.get(old_handle, HandleKind::Listener).unwrap() {
            HandleEntry::Listener(state) => state,
            _ => unreachable!(),
        };
        old_state.operations.start();
        let old_lease = ListenerLease(Arc::clone(&old_state));

        registry.remove(old_handle, HandleKind::Listener).unwrap();
        let replacement_handle = registry.try_insert(listener_entry()).unwrap();
        let replacement_state = match registry
            .get(replacement_handle, HandleKind::Listener)
            .unwrap()
        {
            HandleEntry::Listener(state) => state,
            _ => unreachable!(),
        };

        assert_ne!(old_handle, replacement_handle);
        assert!(old_state.closed.load(Ordering::Acquire));
        assert!(!replacement_state.closed.load(Ordering::Acquire));
        assert_eq!(
            *old_state
                .operations
                .active
                .lock()
                .unwrap_or_else(|err| err.into_inner()),
            1
        );
        assert_eq!(
            *replacement_state
                .operations
                .active
                .lock()
                .unwrap_or_else(|err| err.into_inner()),
            0
        );
        assert!(registry.get(old_handle, HandleKind::Listener).is_none());
        assert!(
            registry
                .get(replacement_handle, HandleKind::Listener)
                .is_some()
        );

        drop(old_lease);
        old_state.wait_for_operations();
        assert_eq!(
            *old_state
                .operations
                .active
                .lock()
                .unwrap_or_else(|err| err.into_inner()),
            0
        );
        assert!(
            registry
                .get(replacement_handle, HandleKind::Listener)
                .is_some()
        );
    }
}
