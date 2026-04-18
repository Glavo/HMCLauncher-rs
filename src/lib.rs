#![allow(non_snake_case)]

#[cfg(not(target_os = "windows"))]
compile_error!("This crate only works on Windows");

mod debug;
mod i18n;
mod java;
mod launcher;
pub mod platform;
mod wide;

/// Mirror HMCL's current minimum required Java major version.
pub(crate) const HMCL_EXPECTED_JAVA_MAJOR_VERSION: u16 = 17;
/// Expose the build-script-generated launcher version string to the runtime.
pub(crate) const HMCL_LAUNCHER_VERSION: &str = env!("HMCL_LAUNCHER_VERSION");

/// Run the launcher and return the process exit code expected by WinMain.
pub fn run() -> i32 {
    launcher::run()
}
