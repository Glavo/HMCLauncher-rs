#![no_std]
#![allow(non_snake_case)]

#[cfg(not(target_os = "windows"))]
compile_error!("This crate only works on Windows");

#[cfg(test)]
extern crate std;

mod debug;
mod heap;
mod i18n;
mod java;
mod launcher;
pub mod platform;
mod wide;

use windows_sys::Win32::System::Threading::ExitProcess;

pub(crate) const HMCL_EXPECTED_JAVA_MAJOR_VERSION: u16 = 17;
pub(crate) const HMCL_LAUNCHER_VERSION: &str = env!("HMCL_LAUNCHER_VERSION");

/// Run the launcher and return the process exit code expected by WinMain.
pub fn run() -> i32 {
    launcher::run()
}

/// Terminate the process without unwinding because the binary uses
/// `panic = "abort"` and no Rust runtime.
pub fn abort(exit_code: u32) -> ! {
    unsafe { ExitProcess(exit_code) }
}
