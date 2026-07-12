use std::ffi::{CStr, OsString, c_char};
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use yar_process_control::{
    CaptureLimits, Deadline, ProcessError, output_with_control, status_with_control,
};

use crate::{YarProcessResult, YarSlice, YarStr};

const HOST_OK: i32 = 0;
const HOST_NOT_FOUND: i32 = 1;
const HOST_PERMISSION_DENIED: i32 = 2;
const HOST_INVALID_ARGUMENT: i32 = 3;
const HOST_IO: i32 = 4;
const HOST_TIMEOUT: i32 = 5;
const HOST_OUTPUT_LIMIT: i32 = 6;
const HOST_CANCELLED: i32 = 7;

const MAX_PROCESS_TIMEOUT_MILLISECONDS: i64 = 86_400_000;
const MAX_CAPTURE_BYTES: i64 = 67_108_864;

static ARGS: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();

pub(crate) fn set_args(argc: i32, argv: *mut *mut c_char) {
    let args = ARGS.get_or_init(|| Mutex::new(Vec::new()));
    let mut values = args.lock().unwrap_or_else(|err| err.into_inner());
    values.clear();

    if argc <= 0 || argv.is_null() {
        return;
    }

    for idx in 0..argc {
        // SAFETY: argv is provided by the native process entry point. A null
        // element is treated like the previous runtime contract: an empty string.
        let arg = unsafe { *argv.add(idx as usize) };
        if arg.is_null() {
            values.push(Vec::new());
            continue;
        }

        // SAFETY: process argv entries are null-terminated C strings.
        values.push(unsafe { CStr::from_ptr(arg) }.to_bytes().to_vec());
    }
}

pub(crate) fn process_args(out: *mut YarSlice) {
    if out.is_null() {
        return;
    }

    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, empty_slice());
    }

    let Some(args) = ARGS.get() else {
        return;
    };
    let values = args.lock().unwrap_or_else(|err| err.into_inner());
    if values.is_empty() {
        return;
    }

    let total = values
        .len()
        .checked_mul(size_of::<YarStr>())
        .and_then(|size| i64::try_from(size).ok())
        .unwrap_or_else(|| super::runtime_fail(b"runtime failure: invalid argv size\n"));
    let ptr = super::yar_alloc_zeroed(total).cast::<YarStr>();

    for (idx, value) in values.iter().enumerate() {
        // SAFETY: ptr points to values.len() YarStr slots allocated above.
        unsafe {
            ptr::write(ptr.add(idx), string_from_bytes(value));
        }
    }

    let len = i32::try_from(values.len())
        .unwrap_or_else(|_| super::runtime_fail(b"runtime failure: invalid argv size\n"));
    // SAFETY: out is an out-pointer from generated code and ptr is runtime-managed.
    unsafe {
        ptr::write(
            out,
            YarSlice {
                ptr: ptr.cast::<u8>(),
                len,
                cap: len,
            },
        );
    }
}

pub(crate) fn env_lookup(name: YarStr, out: *mut YarStr) -> i32 {
    if out.is_null() {
        return HOST_IO;
    }
    // SAFETY: out is an out-pointer from generated code.
    unsafe {
        ptr::write(out, empty_str());
    }

    let Some(name) = checked_string_bytes(name, true) else {
        return HOST_INVALID_ARGUMENT;
    };
    let Ok(name) = std::str::from_utf8(name) else {
        return HOST_INVALID_ARGUMENT;
    };

    let Some(value) = std::env::var_os(name) else {
        return HOST_NOT_FOUND;
    };
    let value = os_string_bytes(value);
    // SAFETY: out is an out-pointer from generated code.
    unsafe {
        ptr::write(out, string_from_bytes(&value));
    }
    HOST_OK
}

pub(crate) fn process_run(
    argv: *const YarSlice,
    timeout_milliseconds: i64,
    max_stdout_bytes: i64,
    max_stderr_bytes: i64,
    cancel: *mut u8,
    out: *mut YarProcessResult,
) -> i32 {
    write_process_result(out, empty_process_result());

    let Some((deadline, capture_limits)) =
        process_limits(timeout_milliseconds, max_stdout_bytes, max_stderr_bytes)
    else {
        return HOST_INVALID_ARGUMENT;
    };
    if super::concurrency::cancellation_requested(cancel).is_none() {
        return HOST_INVALID_ARGUMENT;
    }
    let Some(args) = parse_argv(argv) else {
        return HOST_INVALID_ARGUMENT;
    };

    let mut command = Command::new(&args[0]);
    command.args(&args[1..]);
    match output_with_control(command, deadline, capture_limits, || {
        // A live call keeps the managed channel token rooted. If the registry
        // invariant is nevertheless lost, fail closed and cancel the child.
        super::concurrency::cancellation_requested(cancel).unwrap_or(true)
    }) {
        Ok(output) => {
            let std::process::Output {
                status,
                stdout,
                stderr,
            } = output;
            write_process_result(
                out,
                YarProcessResult {
                    exit_code: exit_code(status),
                    stdout: string_from_bytes(&stdout),
                    stderr: string_from_bytes(&stderr),
                },
            );
            HOST_OK
        }
        Err(error) => status_from_process_error(error),
    }
}

pub(crate) fn process_run_inherit(
    argv: *const YarSlice,
    timeout_milliseconds: i64,
    cancel: *mut u8,
    out: *mut i32,
) -> i32 {
    if out.is_null() {
        return HOST_IO;
    }
    // SAFETY: out is an out-pointer from generated code.
    unsafe {
        ptr::write(out, 0);
    }

    let Some(deadline) = process_deadline(timeout_milliseconds) else {
        return HOST_INVALID_ARGUMENT;
    };
    if super::concurrency::cancellation_requested(cancel).is_none() {
        return HOST_INVALID_ARGUMENT;
    }
    let Some(args) = parse_argv(argv) else {
        return HOST_INVALID_ARGUMENT;
    };

    let mut command = Command::new(&args[0]);
    command
        .args(&args[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match status_with_control(command, deadline, || {
        super::concurrency::cancellation_requested(cancel).unwrap_or(true)
    }) {
        Ok(status) => {
            // SAFETY: out is an out-pointer from generated code.
            unsafe {
                ptr::write(out, exit_code(status));
            }
            HOST_OK
        }
        Err(error) => status_from_process_error(error),
    }
}

fn process_limits(
    timeout_milliseconds: i64,
    max_stdout_bytes: i64,
    max_stderr_bytes: i64,
) -> Option<(Deadline, CaptureLimits)> {
    if !(0..=MAX_CAPTURE_BYTES).contains(&max_stdout_bytes)
        || !(0..=MAX_CAPTURE_BYTES).contains(&max_stderr_bytes)
    {
        return None;
    }
    let deadline = process_deadline(timeout_milliseconds)?;
    let stdout = usize::try_from(max_stdout_bytes).ok()?;
    let stderr = usize::try_from(max_stderr_bytes).ok()?;
    Some((deadline, CaptureLimits::new(stdout, stderr)))
}

fn process_deadline(timeout_milliseconds: i64) -> Option<Deadline> {
    if !(1..=MAX_PROCESS_TIMEOUT_MILLISECONDS).contains(&timeout_milliseconds) {
        return None;
    }
    let timeout = Duration::from_millis(u64::try_from(timeout_milliseconds).ok()?);
    Deadline::after(timeout).ok()
}

fn status_from_process_error(error: ProcessError) -> i32 {
    match error {
        ProcessError::MissingExecutable { .. } => HOST_NOT_FOUND,
        ProcessError::Start { source, .. }
            if source.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            HOST_PERMISSION_DENIED
        }
        ProcessError::Start { source, .. }
            if matches!(
                source.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData
            ) =>
        {
            HOST_INVALID_ARGUMENT
        }
        ProcessError::TimedOut { .. } => HOST_TIMEOUT,
        ProcessError::OutputLimitExceeded { .. } => HOST_OUTPUT_LIMIT,
        ProcessError::Cancelled { .. } => HOST_CANCELLED,
        _ => HOST_IO,
    }
}

fn parse_argv(argv: *const YarSlice) -> Option<Vec<OsString>> {
    if argv.is_null() {
        return None;
    }

    // SAFETY: argv is an immutable pointer from generated code.
    let argv = unsafe { *argv };
    if argv.len <= 0 || argv.cap < argv.len || (argv.ptr.is_null() && argv.len != 0) {
        return None;
    }

    // SAFETY: argv.ptr points to argv.len YarStr values by compiler/runtime ABI.
    let values =
        unsafe { std::slice::from_raw_parts(argv.ptr.cast::<YarStr>(), argv.len as usize) };
    let mut result = Vec::with_capacity(values.len());
    for (idx, value) in values.iter().enumerate() {
        let bytes = checked_string_bytes(*value, idx != 0)?;
        result.push(os_string_from_bytes(bytes.to_vec()));
    }
    Some(result)
}

fn checked_string_bytes(value: YarStr, allow_empty: bool) -> Option<&'static [u8]> {
    if value.len < 0 || (value.ptr.is_null() && value.len != 0) {
        return None;
    }
    if !allow_empty && value.len == 0 {
        return None;
    }
    let bytes = unsafe { string_bytes(value) };
    if bytes.contains(&0) {
        return None;
    }
    Some(bytes)
}

unsafe fn string_bytes<'a>(value: YarStr) -> &'a [u8] {
    if value.len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(value.ptr.cast_const(), value.len as usize) }
}

fn string_from_bytes(value: &[u8]) -> YarStr {
    if value.is_empty() {
        return empty_str();
    }

    let ptr = super::yar_alloc(value.len() as i64);
    // SAFETY: ptr points to value.len() writable bytes allocated above.
    unsafe {
        ptr::copy_nonoverlapping(value.as_ptr(), ptr, value.len());
    }
    YarStr {
        ptr,
        len: value.len() as i64,
    }
}

fn empty_str() -> YarStr {
    YarStr {
        ptr: ptr::null_mut(),
        len: 0,
    }
}

fn empty_process_result() -> YarProcessResult {
    YarProcessResult {
        exit_code: 0,
        stdout: empty_str(),
        stderr: empty_str(),
    }
}

fn write_process_result(out: *mut YarProcessResult, value: YarProcessResult) {
    if out.is_null() {
        return;
    }
    // SAFETY: out is an optional out-pointer from generated code.
    unsafe {
        ptr::write(out, value);
    }
}

fn empty_slice() -> YarSlice {
    YarSlice {
        ptr: ptr::null_mut(),
        len: 0,
        cap: 0,
    }
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }

    exit_code_from_signal(status)
}

#[cfg(unix)]
fn exit_code_from_signal(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;

    status.signal().map_or(HOST_IO, |signal| 128 + signal)
}

#[cfg(not(unix))]
fn exit_code_from_signal(_status: std::process::ExitStatus) -> i32 {
    HOST_IO
}

#[cfg(unix)]
fn os_string_from_bytes(value: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(value)
}

#[cfg(not(unix))]
fn os_string_from_bytes(value: Vec<u8>) -> OsString {
    OsString::from(String::from_utf8_lossy(&value).into_owned())
}

#[cfg(unix)]
fn os_string_bytes(value: std::ffi::OsString) -> Vec<u8> {
    use std::os::unix::ffi::OsStringExt;

    value.into_vec()
}

#[cfg(not(unix))]
fn os_string_bytes(value: std::ffi::OsString) -> Vec<u8> {
    value.to_string_lossy().into_owned().into_bytes()
}
