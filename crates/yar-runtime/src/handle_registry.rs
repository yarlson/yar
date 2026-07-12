use std::{
    collections::BTreeMap,
    fs::File,
    net::{TcpListener, TcpStream},
    ops::Deref,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
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

struct HandleRegistry {
    next_id: i64,
    entries: BTreeMap<i64, HandleEntry>,
}

impl HandleRegistry {
    fn new() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
        }
    }

    fn insert(&mut self, entry: HandleEntry) -> i64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or_else(|| {
            crate::runtime_fail(b"runtime failure: handle registry exhausted\n")
        });
        if self.entries.insert(id, entry).is_some() {
            crate::runtime_fail(b"runtime failure: handle registry corrupted\n");
        }
        id
    }

    fn get(&self, id: i64, kind: HandleKind) -> Option<HandleEntry> {
        if id <= 0 {
            return None;
        }
        self.entries
            .get(&id)
            .filter(|entry| entry.kind() == kind)
            .cloned()
    }

    fn remove(&mut self, id: i64, kind: HandleKind) -> Option<HandleEntry> {
        if self
            .entries
            .get(&id)
            .is_none_or(|entry| entry.kind() != kind)
        {
            return None;
        }
        self.entries.remove(&id)
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
    let HandleEntry::Listener(handle) = registry.get(id, HandleKind::Listener)? else {
        return None;
    };
    handle.closed.store(true, Ordering::Release);
    let _ = registry.remove(id, HandleKind::Listener);
    Some(handle)
}

pub(crate) fn register_connection(stream: TcpStream) -> i64 {
    handles().insert(HandleEntry::Connection(Arc::new(ConnectionState {
        stream,
        closed: AtomicBool::new(false),
        operations: Operations::new(),
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
    let HandleEntry::Connection(handle) = registry.get(id, HandleKind::Connection)? else {
        return None;
    };
    handle.closed.store(true, Ordering::Release);
    let _ = registry.remove(id, HandleKind::Connection);
    Some(handle)
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
