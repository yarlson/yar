use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    io::{self, Read},
    process::{Command, ExitStatus, Output, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use process_wrap::std::{ChildWrapper, CommandWrap};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
#[cfg(unix)]
const TERMINATION_GRACE: Duration = Duration::from_secs(5);
#[cfg(unix)]
const INTERRUPT_GRACE: Duration = Duration::from_secs(1);

#[cfg(unix)]
mod signal_dispatch {
    use std::{
        collections::BTreeMap,
        io,
        sync::{
            Arc, Mutex, OnceLock,
            atomic::{AtomicU64, Ordering},
            mpsc::{self, Receiver, Sender},
        },
        thread,
    };

    use signal_hook::{
        consts::signal::{SIGINT, SIGTERM},
        iterator::Signals,
        low_level::emulate_default_handler,
    };

    static DISPATCHER: OnceLock<Result<Dispatcher, String>> = OnceLock::new();

    struct Dispatcher {
        next_id: AtomicU64,
        subscribers: Arc<Mutex<BTreeMap<u64, Sender<i32>>>>,
    }

    impl Dispatcher {
        fn start() -> Result<Self, String> {
            let subscribers = Arc::new(Mutex::new(BTreeMap::<u64, Sender<i32>>::new()));
            let thread_subscribers = Arc::clone(&subscribers);
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            thread::Builder::new()
                .name("yar-signal-dispatch".to_string())
                .spawn(move || {
                    let mut signals = match Signals::new([SIGINT, SIGTERM]) {
                        Ok(signals) => {
                            let _ = ready_tx.send(Ok(()));
                            signals
                        }
                        Err(error) => {
                            let _ = ready_tx.send(Err(error.to_string()));
                            return;
                        }
                    };
                    for signal in signals.forever() {
                        let senders = thread_subscribers
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        let mut delivered = false;
                        for sender in senders {
                            delivered |= sender.send(signal).is_ok();
                        }
                        if !delivered {
                            let _ = emulate_default_handler(signal);
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
            ready_rx
                .recv()
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            Ok(Self {
                next_id: AtomicU64::new(1),
                subscribers,
            })
        }

        fn subscribe(&self) -> Subscription {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let (sender, receiver) = mpsc::channel();
            self.subscribers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(id, sender);
            Subscription {
                id,
                receiver,
                subscribers: Arc::clone(&self.subscribers),
            }
        }
    }

    pub struct Subscription {
        id: u64,
        receiver: Receiver<i32>,
        subscribers: Arc<Mutex<BTreeMap<u64, Sender<i32>>>>,
    }

    impl Subscription {
        pub fn pending(&self) -> mpsc::TryIter<'_, i32> {
            self.receiver.try_iter()
        }
    }

    impl Drop for Subscription {
        fn drop(&mut self) {
            let idle = {
                let mut subscribers = self
                    .subscribers
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                subscribers.remove(&self.id);
                subscribers.is_empty()
            };
            if idle {
                for signal in self.receiver.try_iter() {
                    let _ = emulate_default_handler(signal);
                }
            }
        }
    }

    pub fn subscribe() -> io::Result<Subscription> {
        match DISPATCHER.get_or_init(Dispatcher::start) {
            Ok(dispatcher) => Ok(dispatcher.subscribe()),
            Err(error) => Err(io::Error::other(error.clone())),
        }
    }
}

#[cfg(windows)]
mod windows_job {
    use std::{
        ffi::c_void,
        io,
        mem::size_of,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Child, Command, ExitStatus},
        thread,
        time::{Duration, Instant},
    };

    use process_wrap::std::{ChildWrapper, CommandWrap, CommandWrapper};
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
                QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
            },
            Threading::{
                CREATE_SUSPENDED, GetProcessId, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
            },
        },
    };

    const JOB_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);
    const JOB_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    #[derive(Debug)]
    struct OwnedHandle(HANDLE);

    // Windows kernel handles can be used from any thread while they remain open.
    unsafe impl Send for OwnedHandle {}
    unsafe impl Sync for OwnedHandle {}

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: this type exclusively owns the valid handle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    #[derive(Debug, Default)]
    pub struct JobObject {
        job: Option<OwnedHandle>,
    }

    impl CommandWrapper for JobObject {
        fn pre_spawn(&mut self, command: &mut Command, _core: &CommandWrap) -> io::Result<()> {
            command.creation_flags(CREATE_SUSPENDED.0);
            Ok(())
        }

        fn post_spawn(
            &mut self,
            _command: &mut Command,
            child: &mut Child,
            _core: &CommandWrap,
        ) -> io::Result<()> {
            match create_job_for(child) {
                Ok(job) => {
                    self.job = Some(job);
                    Ok(())
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(error)
                }
            }
        }

        fn wrap_child(
            &mut self,
            mut inner: Box<dyn ChildWrapper>,
            _core: &CommandWrap,
        ) -> io::Result<Box<dyn ChildWrapper>> {
            let Some(job) = self.job.take() else {
                let _ = inner.kill();
                return Err(io::Error::other("Windows Job Object was not initialized"));
            };
            Ok(Box::new(JobChild {
                job,
                inner,
                leader_status: None,
            }))
        }
    }

    #[derive(Debug)]
    struct JobChild {
        // Keep the job first so kill-on-close runs before the raw Child is dropped.
        job: OwnedHandle,
        inner: Box<dyn ChildWrapper>,
        leader_status: Option<ExitStatus>,
    }

    impl ChildWrapper for JobChild {
        fn inner(&self) -> &dyn ChildWrapper {
            self.inner.inner()
        }

        fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
            self.inner.inner_mut()
        }

        fn into_inner(self: Box<Self>) -> Box<dyn ChildWrapper> {
            let Self { job, inner, .. } = *self;
            drop(job);
            inner
        }

        fn start_kill(&mut self) -> io::Result<()> {
            // SAFETY: the owned handle names a live Job Object.
            unsafe { TerminateJobObject(self.job.0, 1) }.map_err(io::Error::other)
        }

        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            if self.leader_status.is_none() {
                self.leader_status = self.inner.try_wait()?;
            }
            Ok(self.leader_status)
        }

        fn wait(&mut self) -> io::Result<ExitStatus> {
            let expires_at = Instant::now()
                .checked_add(JOB_WAIT_TIMEOUT)
                .expect("short Job Object wait timeout");
            loop {
                if self.leader_status.is_none() {
                    self.leader_status = self.inner.try_wait()?;
                }
                if active_processes(&self.job)? == 0 {
                    let status = match self.leader_status {
                        Some(status) => status,
                        None => self.inner.wait()?,
                    };
                    self.leader_status = Some(status);
                    return Ok(status);
                }
                if Instant::now() >= expires_at {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Windows Job Object still had active processes after termination",
                    ));
                }
                thread::sleep(JOB_WAIT_POLL_INTERVAL);
            }
        }
    }

    fn create_job_for(child: &Child) -> io::Result<OwnedHandle> {
        // SAFETY: null security attributes and name request a private Job Object.
        let job = OwnedHandle(unsafe { CreateJobObjectW(None, None) }.map_err(io::Error::other)?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the pointer and byte length describe `limits` for the requested class.
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(io::Error::other)?;

        let process = HANDLE(child.as_raw_handle());
        // SAFETY: both handles are valid and the child is still suspended.
        unsafe { AssignProcessToJobObject(job.0, process) }.map_err(io::Error::other)?;
        resume_process_threads(process)?;
        Ok(job)
    }

    fn active_processes(job: &OwnedHandle) -> io::Result<u32> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the pointer and byte length describe `accounting` for the requested class.
        unsafe {
            QueryInformationJobObject(
                Some(job.0),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast::<c_void>(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
        }
        .map_err(io::Error::other)?;
        Ok(accounting.ActiveProcesses)
    }

    fn resume_process_threads(process: HANDLE) -> io::Result<()> {
        // SAFETY: the process handle remains valid for the lifetime of `child`.
        let process_id = unsafe { GetProcessId(process) };
        if process_id == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: this requests a system-owned snapshot handle.
        let snapshot = OwnedHandle(
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(io::Error::other)?,
        );
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..THREADENTRY32::default()
        };
        // SAFETY: `entry` has the required size and remains valid through iteration.
        unsafe { Thread32First(snapshot.0, &raw mut entry) }.map_err(io::Error::other)?;
        let mut resumed = false;
        loop {
            if entry.th32OwnerProcessID == process_id {
                // SAFETY: the snapshot supplied this thread ID.
                let thread = OwnedHandle(
                    unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
                        .map_err(io::Error::other)?,
                );
                // SAFETY: the opened handle grants THREAD_SUSPEND_RESUME.
                if unsafe { ResumeThread(thread.0) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                resumed = true;
            }
            // SAFETY: `entry` remains valid and retains its required size.
            if unsafe { Thread32Next(snapshot.0, &raw mut entry) }.is_err() {
                break;
            }
        }
        if !resumed {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no suspended thread found for spawned process",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Timeout(Duration);

impl Timeout {
    pub fn from_env(variable: &str, default_seconds: u64) -> Result<Self, DeadlineError> {
        let value = env::var_os(variable);
        let seconds = match value.as_deref() {
            Some(value) => parse_positive_seconds(variable, value)?,
            None if default_seconds > 0 => default_seconds,
            None => return Err(DeadlineError::InvalidDefault(variable.to_string())),
        };
        let timeout = Duration::from_secs(seconds);
        Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| DeadlineError::InvalidValue {
                variable: variable.to_string(),
                value: value.unwrap_or_else(|| OsString::from(default_seconds.to_string())),
            })?;
        Ok(Self(timeout))
    }

    pub fn start(self) -> Result<Deadline, DeadlineError> {
        Deadline::after(self.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    expires_at: Option<Instant>,
    timeout: Option<Duration>,
}

impl Deadline {
    pub const fn none() -> Self {
        Self {
            expires_at: None,
            timeout: None,
        }
    }

    pub fn after(timeout: Duration) -> Result<Self, DeadlineError> {
        if timeout.is_zero() {
            return Err(DeadlineError::InvalidDuration);
        }
        let Some(expires_at) = Instant::now().checked_add(timeout) else {
            return Err(DeadlineError::InvalidDuration);
        };
        Ok(Self {
            expires_at: Some(expires_at),
            timeout: Some(timeout),
        })
    }

    fn has_limit(self) -> bool {
        self.expires_at.is_some()
    }

    fn expired(self, now: Instant) -> bool {
        self.expires_at.is_some_and(|expires_at| now >= expires_at)
    }

    fn remaining(self, now: Instant) -> Option<Duration> {
        self.expires_at
            .map(|expires_at| expires_at.saturating_duration_since(now))
    }

    fn timeout(self) -> Duration {
        self.timeout.unwrap_or_default()
    }
}

impl Default for Deadline {
    fn default() -> Self {
        Self::none()
    }
}

fn parse_positive_seconds(variable: &str, value: &OsStr) -> Result<u64, DeadlineError> {
    let Some(value_str) = value.to_str() else {
        return Err(DeadlineError::InvalidValue {
            variable: variable.to_string(),
            value: value.to_owned(),
        });
    };
    match value_str.parse::<u64>() {
        Ok(seconds) if seconds > 0 => Ok(seconds),
        _ => Err(DeadlineError::InvalidValue {
            variable: variable.to_string(),
            value: value.to_owned(),
        }),
    }
}

#[derive(Debug)]
pub enum DeadlineError {
    InvalidDuration,
    InvalidDefault(String),
    InvalidValue { variable: String, value: OsString },
}

impl fmt::Display for DeadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeadlineError::InvalidDuration => {
                f.write_str("process timeout must be positive and fit the host clock")
            }
            DeadlineError::InvalidDefault(variable) => {
                write!(
                    f,
                    "internal timeout default for {variable} must be positive"
                )
            }
            DeadlineError::InvalidValue { variable, value } => write!(
                f,
                "{variable} must be a positive integer number of seconds, got {:?}",
                value.to_string_lossy()
            ),
        }
    }
}

impl Error for DeadlineError {}

#[derive(Debug)]
pub enum ProcessError {
    MissingExecutable {
        program: OsString,
        source: io::Error,
    },
    Start {
        program: OsString,
        source: io::Error,
    },
    Wait {
        program: OsString,
        source: io::Error,
    },
    Terminate {
        program: OsString,
        source: io::Error,
    },
    Capture {
        program: OsString,
        stream: &'static str,
        source: io::Error,
    },
    TimedOut {
        program: OsString,
        timeout: Duration,
    },
    #[cfg(unix)]
    Interrupted { program: OsString, signal: i32 },
    #[cfg(unix)]
    SignalSetup {
        program: OsString,
        source: io::Error,
    },
    #[cfg(unix)]
    SignalForward {
        program: OsString,
        signal: i32,
        source: io::Error,
    },
}

impl ProcessError {
    pub fn interrupted_signal(&self) -> Option<i32> {
        match self {
            #[cfg(unix)]
            ProcessError::Interrupted { signal, .. } => Some(*signal),
            _ => None,
        }
    }
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::MissingExecutable { program, .. } => write!(
                f,
                "required executable {:?} was not found; verify its path or install it in PATH",
                program.to_string_lossy()
            ),
            ProcessError::Start { program, source } => write!(
                f,
                "starting process {:?}: {source}",
                program.to_string_lossy()
            ),
            ProcessError::Wait { program, source } => write!(
                f,
                "waiting for process {:?}: {source}",
                program.to_string_lossy()
            ),
            ProcessError::Terminate { program, source } => write!(
                f,
                "terminating process {:?}: {source}",
                program.to_string_lossy()
            ),
            ProcessError::Capture {
                program,
                stream,
                source,
            } => write!(
                f,
                "reading {stream} from process {:?}: {source}",
                program.to_string_lossy()
            ),
            ProcessError::TimedOut { program, timeout } => write!(
                f,
                "operation running {:?} timed out after {}",
                program.to_string_lossy(),
                format_duration(*timeout)
            ),
            #[cfg(unix)]
            ProcessError::Interrupted { program, signal } => write!(
                f,
                "process {:?} was interrupted by signal {signal}",
                program.to_string_lossy()
            ),
            #[cfg(unix)]
            ProcessError::SignalSetup { program, source } => write!(
                f,
                "installing signal forwarding for process {:?}: {source}",
                program.to_string_lossy()
            ),
            #[cfg(unix)]
            ProcessError::SignalForward {
                program,
                signal,
                source,
            } => write!(
                f,
                "forwarding signal {signal} to process {:?}: {source}",
                program.to_string_lossy()
            ),
        }
    }
}

impl Error for ProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ProcessError::MissingExecutable { source, .. }
            | ProcessError::Start { source, .. }
            | ProcessError::Wait { source, .. }
            | ProcessError::Terminate { source, .. }
            | ProcessError::Capture { source, .. } => Some(source),
            ProcessError::TimedOut { .. } => None,
            #[cfg(unix)]
            ProcessError::Interrupted { .. } => None,
            #[cfg(unix)]
            ProcessError::SignalSetup { source, .. }
            | ProcessError::SignalForward { source, .. } => Some(source),
        }
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.subsec_nanos() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{duration:?}")
    }
}

pub fn output(mut command: Command, deadline: Deadline) -> Result<Output, ProcessError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let program = command.get_program().to_owned();
    let mut child = spawn(command, deadline, &program)?;
    let stdout = spawn_reader(child.child_mut().stdout().take(), "stdout", &program)?;
    let stderr = match spawn_reader(child.child_mut().stderr().take(), "stderr", &program) {
        Ok(stderr) => stderr,
        Err(error) => {
            let _ = child.terminate(&program);
            drop(stdout);
            return Err(error);
        }
    };

    let status = match wait(&mut child, deadline, &program) {
        Ok(status) => status,
        Err(error) => {
            drop(stdout);
            drop(stderr);
            return Err(error);
        }
    };
    let stdout = join_reader(stdout, "stdout", &program, deadline, &mut child)?;
    let stderr = join_reader(stderr, "stderr", &program, deadline, &mut child)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub fn status(command: Command, deadline: Deadline) -> Result<ExitStatus, ProcessError> {
    let program = command.get_program().to_owned();
    let mut child = spawn(command, deadline, &program)?;
    wait(&mut child, deadline, &program)
}

fn spawn(
    command: Command,
    deadline: Deadline,
    program: &OsStr,
) -> Result<ChildGuard, ProcessError> {
    if deadline.expired(Instant::now()) {
        return Err(ProcessError::TimedOut {
            program: program.to_owned(),
            timeout: deadline.timeout(),
        });
    }

    #[cfg(unix)]
    let signals = if deadline.has_limit() {
        Some(
            signal_dispatch::subscribe().map_err(|source| ProcessError::SignalSetup {
                program: program.to_owned(),
                source,
            })?,
        )
    } else {
        None
    };

    #[cfg(unix)]
    let command = {
        let mut command = command;
        if deadline.has_limit() {
            command.process_group(0);
        }
        command
    };

    let mut command = CommandWrap::from(command);
    if deadline.has_limit() {
        #[cfg(windows)]
        command.wrap(windows_job::JobObject::default());
    }
    let child = command
        .spawn()
        .map_err(|source| spawn_error(program, source))?;
    #[cfg(unix)]
    let process_group = deadline
        .has_limit()
        .then(|| i32::try_from(child.id()).expect("process ID exceeds i32"));
    Ok(ChildGuard {
        child: Some(child),
        #[cfg(unix)]
        process_group,
        #[cfg(unix)]
        signals,
    })
}

fn spawn_error(program: &OsStr, source: io::Error) -> ProcessError {
    if source.kind() == io::ErrorKind::NotFound {
        ProcessError::MissingExecutable {
            program: program.to_owned(),
            source,
        }
    } else {
        ProcessError::Start {
            program: program.to_owned(),
            source,
        }
    }
}

fn wait(
    child: &mut ChildGuard,
    deadline: Deadline,
    program: &OsStr,
) -> Result<ExitStatus, ProcessError> {
    if !deadline.has_limit() {
        return child.wait(program);
    }

    #[cfg(unix)]
    let mut interruption: Option<(i32, Instant)> = None;
    loop {
        let now = Instant::now();

        #[cfg(unix)]
        let pending_signals = child
            .signals
            .as_ref()
            .map(|signals| signals.pending().collect::<Vec<_>>())
            .unwrap_or_default();
        #[cfg(unix)]
        for signal in pending_signals {
            if let Some((original_signal, _)) = interruption {
                child.terminate(program)?;
                return Err(ProcessError::Interrupted {
                    program: program.to_owned(),
                    signal: original_signal,
                });
            }
            if let Err(source) = child.signal_process_group(signal)
                && source.raw_os_error() != Some(libc::ESRCH)
            {
                return Err(ProcessError::SignalForward {
                    program: program.to_owned(),
                    signal,
                    source,
                });
            }
            interruption = Some((signal, now + INTERRUPT_GRACE));
        }

        if let Some(status) = child.try_wait(program)? {
            child.finish(program, deadline)?;
            #[cfg(unix)]
            if let Some((signal, _)) = interruption {
                return Err(ProcessError::Interrupted {
                    program: program.to_owned(),
                    signal,
                });
            }
            return Ok(status);
        }

        #[cfg(unix)]
        if let Some((signal, expires_at)) = interruption
            && now >= expires_at
        {
            child.terminate(program)?;
            return Err(ProcessError::Interrupted {
                program: program.to_owned(),
                signal,
            });
        }

        if deadline.expired(now) {
            child.terminate(program)?;
            return Err(ProcessError::TimedOut {
                program: program.to_owned(),
                timeout: deadline.timeout(),
            });
        }

        let sleep_for = deadline.remaining(now).unwrap_or(POLL_INTERVAL);
        #[cfg(unix)]
        let sleep_for = interruption.map_or(sleep_for, |(_, expires_at)| {
            sleep_for.min(expires_at.saturating_duration_since(now))
        });
        thread::sleep(sleep_for.min(POLL_INTERVAL));
    }
}

fn spawn_reader<R>(
    pipe: Option<R>,
    stream: &'static str,
    program: &OsStr,
) -> Result<Option<JoinHandle<io::Result<Vec<u8>>>>, ProcessError>
where
    R: Read + Send + 'static,
{
    let Some(mut pipe) = pipe else {
        return Ok(None);
    };
    thread::Builder::new()
        .name(format!("yar-{stream}-reader"))
        .spawn(move || {
            let mut output = Vec::new();
            pipe.read_to_end(&mut output)?;
            Ok(output)
        })
        .map(Some)
        .map_err(|source| ProcessError::Capture {
            program: program.to_owned(),
            stream,
            source,
        })
}

fn join_reader(
    reader: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stream: &'static str,
    program: &OsStr,
    deadline: Deadline,
    _child: &mut ChildGuard,
) -> Result<Vec<u8>, ProcessError> {
    let Some(reader) = reader else {
        return Ok(Vec::new());
    };
    while !reader.is_finished() {
        #[cfg(unix)]
        if let Some(signal) = _child.next_signal() {
            return Err(ProcessError::Interrupted {
                program: program.to_owned(),
                signal,
            });
        }
        let now = Instant::now();
        if deadline.expired(now) {
            return Err(ProcessError::TimedOut {
                program: program.to_owned(),
                timeout: deadline.timeout(),
            });
        }
        thread::sleep(
            deadline
                .remaining(now)
                .unwrap_or(POLL_INTERVAL)
                .min(POLL_INTERVAL),
        );
    }
    match reader.join() {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(source)) => Err(ProcessError::Capture {
            program: program.to_owned(),
            stream,
            source,
        }),
        Err(_) => Err(ProcessError::Capture {
            program: program.to_owned(),
            stream,
            source: io::Error::other("reader thread panicked"),
        }),
    }
}

struct ChildGuard {
    child: Option<Box<dyn ChildWrapper>>,
    #[cfg(unix)]
    process_group: Option<i32>,
    #[cfg(unix)]
    signals: Option<signal_dispatch::Subscription>,
}

impl ChildGuard {
    fn child_mut(&mut self) -> &mut dyn ChildWrapper {
        self.child.as_deref_mut().expect("live child")
    }

    #[cfg(unix)]
    fn next_signal(&self) -> Option<i32> {
        self.signals.as_ref()?.pending().next()
    }

    #[cfg(unix)]
    fn signal_process_group(&self, signal: i32) -> io::Result<()> {
        let process_group = self.process_group.expect("timed Unix process group");
        // SAFETY: a negative PID targets only the operation-owned process group.
        if unsafe { libc::kill(-process_group, signal) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn try_wait(&mut self, program: &OsStr) -> Result<Option<ExitStatus>, ProcessError> {
        match self.child_mut().try_wait() {
            Ok(Some(status)) => Ok(Some(status)),
            Ok(None) => Ok(None),
            Err(source) => {
                self.terminate_best_effort();
                Err(ProcessError::Wait {
                    program: program.to_owned(),
                    source,
                })
            }
        }
    }

    fn wait(&mut self, program: &OsStr) -> Result<ExitStatus, ProcessError> {
        match self.child_mut().wait() {
            Ok(status) => {
                self.child = None;
                Ok(status)
            }
            Err(source) => {
                self.terminate_best_effort();
                Err(ProcessError::Wait {
                    program: program.to_owned(),
                    source,
                })
            }
        }
    }

    #[cfg(unix)]
    fn finish(&mut self, program: &OsStr, _deadline: Deadline) -> Result<(), ProcessError> {
        let Some(child) = self.child.take() else {
            return Ok(());
        };

        if let Err(source) = self.signal_process_group(libc::SIGKILL)
            && source.raw_os_error() != Some(libc::ESRCH)
        {
            self.child = Some(child);
            return Err(ProcessError::Terminate {
                program: program.to_owned(),
                source,
            });
        }
        drop(child);
        self.wait_for_process_group(program, termination_deadline(program)?)
    }

    #[cfg(windows)]
    fn finish(&mut self, program: &OsStr, _deadline: Deadline) -> Result<(), ProcessError> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if let Err(source) = child.start_kill().and_then(|()| child.wait().map(drop)) {
            self.child = Some(child);
            return Err(ProcessError::Terminate {
                program: program.to_owned(),
                source,
            });
        }
        Ok(())
    }

    #[cfg(unix)]
    fn terminate(&mut self, program: &OsStr) -> Result<(), ProcessError> {
        if self.child.is_none() {
            return Ok(());
        }

        if let Err(source) = self.signal_process_group(libc::SIGKILL) {
            if source.raw_os_error() == Some(libc::ESRCH) {
                // The leader won the exit race; it still needs to be waited.
            } else {
                return Err(ProcessError::Terminate {
                    program: program.to_owned(),
                    source,
                });
            }
        }
        let child = self.child.as_deref_mut().expect("live child");
        if let Err(source) = child.start_kill()
            && source.raw_os_error() != Some(libc::ESRCH)
        {
            return Err(ProcessError::Terminate {
                program: program.to_owned(),
                source,
            });
        }

        let cleanup_deadline = termination_deadline(program)?;
        loop {
            match self.child_mut().try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(source) => {
                    return Err(ProcessError::Terminate {
                        program: program.to_owned(),
                        source,
                    });
                }
            }
            if cleanup_deadline.expired(Instant::now()) {
                return Err(ProcessError::Terminate {
                    program: program.to_owned(),
                    source: io::Error::new(
                        io::ErrorKind::TimedOut,
                        "processes did not exit after termination",
                    ),
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
        self.child = None;

        self.wait_for_process_group(program, cleanup_deadline)?;
        Ok(())
    }

    #[cfg(windows)]
    fn terminate(&mut self, program: &OsStr) -> Result<(), ProcessError> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if let Err(source) = child.start_kill().and_then(|()| child.wait().map(drop)) {
            self.child = Some(child);
            return Err(ProcessError::Terminate {
                program: program.to_owned(),
                source,
            });
        }
        Ok(())
    }

    #[cfg(unix)]
    fn wait_for_process_group(
        &mut self,
        program: &OsStr,
        deadline: Deadline,
    ) -> Result<(), ProcessError> {
        let Some(process_group) = self.process_group else {
            return Ok(());
        };
        let mut interrupted = None;
        loop {
            reap_process_group(process_group).map_err(|source| ProcessError::Wait {
                program: program.to_owned(),
                source,
            })?;
            if !process_group_exists(process_group).map_err(|source| ProcessError::Terminate {
                program: program.to_owned(),
                source,
            })? {
                self.process_group = None;
                return match interrupted {
                    Some(signal) => Err(ProcessError::Interrupted {
                        program: program.to_owned(),
                        signal,
                    }),
                    None => Ok(()),
                };
            }
            if interrupted.is_none() {
                interrupted = self.next_signal();
            }
            if deadline.expired(Instant::now()) {
                return Err(ProcessError::Terminate {
                    program: program.to_owned(),
                    source: io::Error::new(
                        io::ErrorKind::TimedOut,
                        "process group did not exit after termination",
                    ),
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn terminate_best_effort(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        #[cfg(unix)]
        let _ = self.signal_process_group(libc::SIGKILL);
        let _ = child.start_kill();
        let _ = child.wait();

        #[cfg(unix)]
        if let Some(process_group) = self.process_group.take() {
            // SAFETY: negative PID targets only the operation-owned process group.
            let _ = unsafe { libc::kill(-process_group, libc::SIGKILL) };
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.terminate_best_effort();
    }
}

#[cfg(unix)]
fn termination_deadline(program: &OsStr) -> Result<Deadline, ProcessError> {
    Deadline::after(TERMINATION_GRACE).map_err(|source| ProcessError::Terminate {
        program: program.to_owned(),
        source: io::Error::other(source),
    })
}

#[cfg(unix)]
fn reap_process_group(process_group: i32) -> io::Result<()> {
    loop {
        let mut status = 0;
        // SAFETY: WNOHANG makes this a nonblocking reap of adopted group children.
        let result = unsafe { libc::waitpid(-process_group, &raw mut status, libc::WNOHANG) };
        if result > 0 {
            continue;
        }
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::ECHILD) => return Ok(()),
            Some(libc::EINTR) => continue,
            _ => return Err(error),
        }
    }
}

#[cfg(unix)]
fn process_group_exists(process_group: i32) -> io::Result<bool> {
    // SAFETY: signal 0 performs an existence/permission check without delivery.
    if unsafe { libc::kill(-process_group, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn process_probe() {
        match env::var("YAR_PROCESS_CONTROL_PROBE").as_deref() {
            Ok("output") => {
                let bytes = vec![b'x'; 256 * 1024];
                io::stdout().write_all(&bytes).unwrap();
                io::stderr().write_all(&bytes).unwrap();
            }
            Ok("sleep") => thread::sleep(Duration::from_secs(30)),
            #[cfg(unix)]
            Ok("descendant") => {
                let pid_path = env::var_os("YAR_DESCENDANT_PID").unwrap();
                let mut descendant = Command::new("sh").args(["-c", "sleep 30"]).spawn().unwrap();
                fs::write(pid_path, descendant.id().to_string()).unwrap();
                descendant.wait().unwrap();
            }
            #[cfg(unix)]
            Ok("detached-descendant") => {
                let pid_path = env::var_os("YAR_DESCENDANT_PID").unwrap();
                // The probe must exit first so the outer runner can prove that
                // it terminates remaining members of the process group.
                #[allow(clippy::zombie_processes)]
                let descendant = Command::new("sh").args(["-c", "sleep 30"]).spawn().unwrap();
                fs::write(pid_path, descendant.id().to_string()).unwrap();
            }
            #[cfg(windows)]
            Ok("descendant") => {
                let marker = env::var_os("YAR_DESCENDANT_MARKER").unwrap();
                let mut descendant = probe_command("mark-after-sleep");
                descendant.env("YAR_DESCENDANT_MARKER", marker);
                descendant.spawn().unwrap().wait().unwrap();
            }
            #[cfg(windows)]
            Ok("detached-descendant") => {
                let marker = env::var_os("YAR_DESCENDANT_MARKER").unwrap();
                let mut descendant = probe_command("mark-after-sleep");
                descendant.env("YAR_DESCENDANT_MARKER", marker);
                #[allow(clippy::zombie_processes)]
                descendant.spawn().unwrap();
            }
            #[cfg(windows)]
            Ok("mark-after-sleep") => {
                thread::sleep(Duration::from_secs(2));
                fs::write(env::var_os("YAR_DESCENDANT_MARKER").unwrap(), b"done").unwrap();
            }
            #[cfg(unix)]
            Ok("default-signal-after-timed") => {
                status(
                    Command::new("true"),
                    Deadline::after(Duration::from_secs(5)).unwrap(),
                )
                .unwrap();
                // SAFETY: raising SIGTERM in this probe is the behavior under test.
                unsafe { libc::raise(libc::SIGTERM) };
                thread::sleep(Duration::from_secs(30));
            }
            #[cfg(unix)]
            Ok("forward-signal") => {
                let error = status(
                    probe_command("sleep"),
                    Deadline::after(Duration::from_secs(30)).unwrap(),
                )
                .unwrap_err();
                assert_eq!(error.interrupted_signal(), Some(libc::SIGTERM));
                std::process::exit(128 + libc::SIGTERM);
            }
            #[cfg(target_os = "linux")]
            Ok("subreaper-timeout") => {
                // SAFETY: this affects only the isolated probe process.
                assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
                let mut command = probe_command("descendant");
                command.env(
                    "YAR_DESCENDANT_PID",
                    env::var_os("YAR_DESCENDANT_PID").unwrap(),
                );
                let error = status(
                    command,
                    Deadline::after(Duration::from_millis(200)).unwrap(),
                )
                .unwrap_err();
                assert!(matches!(error, ProcessError::TimedOut { .. }));
            }
            _ => {}
        }
    }

    #[test]
    fn captures_large_stdout_and_stderr_without_deadlock() {
        let output = output(
            probe_command("output"),
            Deadline::after(Duration::from_secs(5)).unwrap(),
        )
        .unwrap();

        assert!(output.status.success());
        assert!(output.stdout.len() >= 256 * 1024);
        assert!(output.stderr.len() >= 256 * 1024);
    }

    #[test]
    fn timeout_terminates_and_reaps_the_process() {
        let error = status(
            probe_command("sleep"),
            Deadline::after(Duration::from_millis(100)).unwrap(),
        )
        .unwrap_err();

        assert!(matches!(error, ProcessError::TimedOut { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_descendants_in_the_process_group() {
        let dir = TempDir::new("yar-process-descendant");
        let pid_path = dir.path().join("pid");
        let mut command = probe_command("descendant");
        command.env("YAR_DESCENDANT_PID", &pid_path);

        let error = status(command, Deadline::after(Duration::from_secs(1)).unwrap()).unwrap_err();
        assert!(matches!(error, ProcessError::TimedOut { .. }));

        let pid = fs::read_to_string(&pid_path).unwrap();
        let status = Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "descendant {pid} survived timeout");
    }

    #[cfg(unix)]
    #[test]
    fn successful_parent_exit_does_not_leave_group_descendants_running() {
        let dir = TempDir::new("yar-process-success-descendant");
        let pid_path = dir.path().join("pid");
        let mut command = probe_command("detached-descendant");
        command.env("YAR_DESCENDANT_PID", &pid_path);

        let status = status(command, Deadline::after(Duration::from_secs(5)).unwrap()).unwrap();
        assert!(status.success());

        let pid = fs::read_to_string(&pid_path).unwrap();
        let status = Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "descendant {pid} survived parent exit");
    }

    #[cfg(windows)]
    #[test]
    fn timeout_terminates_descendants_in_the_job() {
        let dir = TempDir::new("yar-process-windows-timeout-descendant");
        let marker = dir.path().join("marker");
        let mut command = probe_command("descendant");
        command.env("YAR_DESCENDANT_MARKER", &marker);

        let error = status(
            command,
            Deadline::after(Duration::from_millis(200)).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(error, ProcessError::TimedOut { .. }));
        thread::sleep(Duration::from_millis(2_200));
        assert!(!marker.exists(), "descendant survived Job Object timeout");
    }

    #[cfg(windows)]
    #[test]
    fn successful_parent_exit_terminates_remaining_job_descendants() {
        let dir = TempDir::new("yar-process-windows-success-descendant");
        let marker = dir.path().join("marker");
        let mut command = probe_command("detached-descendant");
        command.env("YAR_DESCENDANT_MARKER", &marker);

        let status = status(command, Deadline::after(Duration::from_secs(5)).unwrap()).unwrap();
        assert!(status.success());
        thread::sleep(Duration::from_millis(2_200));
        assert!(!marker.exists(), "descendant survived parent exit");
    }

    #[cfg(unix)]
    #[test]
    fn default_termination_still_works_after_a_timed_process() {
        use std::os::unix::process::ExitStatusExt;

        let status = probe_command("default-signal-after-timed")
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }

    #[cfg(unix)]
    #[test]
    fn termination_signals_are_forwarded_to_a_timed_process_group() {
        let mut command = probe_command("forward-signal").spawn().unwrap();
        thread::sleep(Duration::from_millis(250));
        // SAFETY: the PID belongs to the live probe child.
        assert_eq!(unsafe { libc::kill(command.id() as i32, libc::SIGTERM) }, 0);
        let status = command.wait().unwrap();
        assert_eq!(status.code(), Some(128 + libc::SIGTERM));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn subreaper_reaps_adopted_process_group_descendants() {
        let dir = TempDir::new("yar-process-subreaper-descendant");
        let mut command = probe_command("subreaper-timeout");
        command.env("YAR_DESCENDANT_PID", dir.path().join("pid"));

        let status = command.status().unwrap();
        assert!(status.success());
    }

    #[test]
    fn missing_executable_error_names_the_program() {
        let program = format!("yar-definitely-missing-{}", std::process::id());
        let error = output(
            Command::new(&program),
            Deadline::after(Duration::from_secs(1)).unwrap(),
        )
        .unwrap_err();

        assert!(matches!(error, ProcessError::MissingExecutable { .. }));
        assert!(error.to_string().contains(&program));
    }

    #[test]
    fn rejects_invalid_timeout_values() {
        for value in ["", "0", "-1", "1.5", "forever"] {
            assert!(parse_positive_seconds("TIMEOUT", OsStr::new(value)).is_err());
        }
        assert_eq!(
            parse_positive_seconds("TIMEOUT", OsStr::new("12")).unwrap(),
            12
        );
    }

    fn probe_command(mode: &str) -> Command {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--exact", "tests::process_probe", "--nocapture"])
            .env("YAR_PROCESS_CONTROL_PROBE", mode);
        command
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("{prefix}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
