#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

#[cfg(not(target_os = "windows"))]
compile_error!("This crate only works on Windows");

extern crate HMCLauncher as hmclauncher;

/// Request the high-performance NVIDIA GPU on dual-GPU systems.
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static NvOptimusEnablement: u32 = 1;

/// Request the high-performance AMD GPU on dual-GPU systems.
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// Enter the launcher through Rust's normal `std` runtime entrypoint.
fn main() {
    std::process::exit(hmclauncher::run());
}
