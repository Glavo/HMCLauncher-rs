#![no_std]
#![no_main]
#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

#[cfg(not(target_os = "windows"))]
compile_error!("This crate only works on Windows");

extern crate HMCLauncher as hmclauncher;

use core::panic::PanicInfo;
use windows_sys::Win32::Foundation::HINSTANCE;

/// Request the high-performance NVIDIA GPU on dual-GPU systems.
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static NvOptimusEnablement: u32 = 1;

/// Request the high-performance AMD GPU on dual-GPU systems.
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// Abort immediately because a `no_std` GUI binary cannot surface panic details
/// usefully to end users.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    hmclauncher::abort(101)
}

// Export both GUI entrypoint variants so the MinGW CRT can resolve the one it
// expects without pulling in Rust's normal main wrapper.
/// Enter the launcher through the wide-character GUI entrypoint.
#[unsafe(no_mangle)]
pub extern "system" fn wWinMain(
    _instance: HINSTANCE,
    _previous: HINSTANCE,
    _command_line: *mut u16,
    _show: i32,
) -> i32 {
    hmclauncher::run()
}

/// Enter the launcher through the ANSI GUI entrypoint when that is what the
/// CRT expects.
#[unsafe(no_mangle)]
pub extern "system" fn WinMain(
    _instance: HINSTANCE,
    _previous: HINSTANCE,
    _command_line: *mut u8,
    _show: i32,
) -> i32 {
    hmclauncher::run()
}
