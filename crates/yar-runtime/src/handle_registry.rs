use std::{
    collections::BTreeMap,
    fs::File,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

pub(crate) type FileHandle = Arc<Mutex<Option<File>>>;
pub(crate) type ListenerHandle = Arc<Mutex<Option<TcpListener>>>;
pub(crate) type ConnectionHandle = Arc<Mutex<Option<TcpStream>>>;
pub(crate) type StringBuilderHandle = Arc<Mutex<Vec<u8>>>;

#[derive(Clone)]
enum HandleEntry {
    File(FileHandle),
    Listener(ListenerHandle),
    Connection(ConnectionHandle),
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
    handles().insert(HandleEntry::Listener(Arc::new(Mutex::new(Some(listener)))))
}

pub(crate) fn listener(id: i64) -> Option<ListenerHandle> {
    match handles().get(id, HandleKind::Listener)? {
        HandleEntry::Listener(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn remove_listener(id: i64) -> Option<ListenerHandle> {
    match handles().remove(id, HandleKind::Listener)? {
        HandleEntry::Listener(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn register_connection(stream: TcpStream) -> i64 {
    handles().insert(HandleEntry::Connection(Arc::new(Mutex::new(Some(stream)))))
}

pub(crate) fn connection(id: i64) -> Option<ConnectionHandle> {
    match handles().get(id, HandleKind::Connection)? {
        HandleEntry::Connection(handle) => Some(handle),
        _ => None,
    }
}

pub(crate) fn remove_connection(id: i64) -> Option<ConnectionHandle> {
    match handles().remove(id, HandleKind::Connection)? {
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
