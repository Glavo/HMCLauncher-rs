#![no_std]
#![no_main]
#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

#[cfg(not(target_os = "windows"))]
compile_error!("This crate only works on Windows");

extern crate HMCLauncher as hmclauncher;

use core::panic::PanicInfo;
use windows_sys::Win32::Foundation::HINSTANCE;

#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static NvOptimusEnablement: u32 = 1;

#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    hmclauncher::abort(101)
}

#[unsafe(no_mangle)]
pub extern "system" fn wWinMain(
    _instance: HINSTANCE,
    _previous: HINSTANCE,
    _command_line: *mut u16,
    _show: i32,
) -> i32 {
    hmclauncher::run()
}

#[unsafe(no_mangle)]
pub extern "system" fn WinMain(
    _instance: HINSTANCE,
    _previous: HINSTANCE,
    _command_line: *mut u8,
    _show: i32,
) -> i32 {
    hmclauncher::run()
}
