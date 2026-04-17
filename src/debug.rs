use core::fmt::{self, Write};
use core::mem::zeroed;
use core::ptr;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, SYSTEMTIME};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE, WriteConsoleW,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::wide::{WideDisplay, WideString};

/// Cache whether verbose diagnostic output should be emitted.
static mut VERBOSE_OUTPUT: bool = false;
/// Cache the attached console output handle after successful attachment.
static mut CONSOLE_OUTPUT: HANDLE = ptr::null_mut();

/// Record whether verbose logging should be emitted for discovery steps.
pub fn set_verbose_output(value: bool) {
    unsafe {
        VERBOSE_OUTPUT = value;
    }
}

/// Return the current verbose logging flag.
pub fn verbose_output() -> bool {
    unsafe { VERBOSE_OUTPUT }
}

/// Attach to the parent console so the launcher behaves like the upstream
/// executable when started from a shell.
pub fn attach_console() -> bool {
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    if !attached {
        return false;
    }

    // Cache the output handle once so the rest of the launcher can log without
    // repeating console discovery on every line.
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return false;
    }

    unsafe {
        CONSOLE_OUTPUT = handle;
    }
    let newline = [b'\n' as u16];
    write_console_line(&newline);
    true
}

/// Format a UTF-8 message into UTF-16 and send it to the attached console.
pub fn log_fmt(args: fmt::Arguments<'_>) {
    let mut message = WideString::new();
    if message.write_fmt(args).is_ok() {
        log_wide(message.as_slice());
    }
}

/// Emit a formatted message only when verbose logging is enabled.
pub fn log_verbose_fmt(args: fmt::Arguments<'_>) {
    if verbose_output() {
        log_fmt(args);
    }
}

/// Write a prebuilt UTF-16 message with the launcher's timestamped prefix.
pub fn log_wide(message: &[u16]) {
    let handle = unsafe { CONSOLE_OUTPUT };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return;
    }

    let mut time: SYSTEMTIME = unsafe { zeroed() };
    unsafe {
        GetLocalTime(&mut time);
    }

    let mut line = WideString::new();
    // Build the full line in memory first so the console sees a single write.
    if write!(
        &mut line,
        "[{:02}:{:02}:{:02}] [HMCLauncher] {}",
        time.wHour,
        time.wMinute,
        time.wSecond,
        WideDisplay(message)
    )
    .is_err()
    {
        return;
    }

    let _ = line.push_str("\r\n");
    let mut written = 0u32;
    unsafe {
        WriteConsoleW(
            handle,
            line.as_pcwstr(),
            line.len() as u32,
            &mut written,
            ptr::null(),
        );
    }
}

/// Write a raw UTF-16 buffer to the cached console handle.
fn write_console_line(message: &[u16]) {
    let handle = unsafe { CONSOLE_OUTPUT };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE || message.is_empty() {
        return;
    }

    let mut written = 0u32;
    unsafe {
        WriteConsoleW(
            handle,
            message.as_ptr(),
            message.len() as u32,
            &mut written,
            ptr::null(),
        );
    }
}
