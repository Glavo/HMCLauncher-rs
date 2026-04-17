use core::fmt::Write;
use core::ptr;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MessageBoxW, SW_SHOW,
};
use windows_sys::core::w;

use crate::HMCL_LAUNCHER_VERSION;
use crate::debug::{attach_console, log_fmt, log_verbose_fmt, set_verbose_output};
use crate::i18n::current as current_i18n;
use crate::java::{
    JavaList, JavaOptions, launch_jvm, search_java_in_dir, search_java_in_path,
    search_java_in_program_files, search_java_in_registry,
};
use crate::platform::{Arch, get_env_path, get_env_var, get_self_path, is_regular_file};
use crate::wide::{WideDisplay, WideString};

/// Execute the launcher flow and return a WinMain-style exit code.
pub fn run() -> i32 {
    let verbose_output = !matches!(
        get_env_var(w!("HMCL_LAUNCHER_VERBOSE_OUTPUT")),
        Some(value) if value.equals_str("false")
    );
    set_verbose_output(verbose_output);

    let java_executable_name = if attach_console() {
        "java.exe"
    } else {
        "javaw.exe"
    };

    let arch = Arch::current();
    let i18n = current_i18n();

    let Some(self_path) = get_self_path() else {
        log_fmt(format_args!("Failed to get self path"));
        unsafe {
            MessageBoxW(
                ptr::null_mut(),
                i18n.error_self_path,
                ptr::null(),
                MB_OK | MB_ICONERROR,
            );
        }
        return 1;
    };

    let options = JavaOptions {
        workdir: self_path.workdir,
        jar_path: self_path.jar_path,
        jvm_options: get_env_var(w!("HMCL_JAVA_OPTS")),
    };

    log_fmt(format_args!(
        "*** HMCL Launcher {} ***",
        HMCL_LAUNCHER_VERSION
    ));
    log_fmt(format_args!("System Architecture: {}", arch.display_name()));
    log_fmt(format_args!(
        "Working directory: {}",
        WideDisplay(options.workdir.as_slice())
    ));
    log_fmt(format_args!(
        "Exe File: {}\\{}",
        WideDisplay(options.workdir.as_slice()),
        WideDisplay(options.jar_path.as_slice())
    ));
    if let Some(jvm_options) = options.jvm_options.as_ref() {
        log_fmt(format_args!(
            "JVM Options: {}",
            WideDisplay(jvm_options.as_slice())
        ));
    }

    // Search order intentionally matches the original launcher so bundled and
    // HMCL-managed runtimes win before wider system discovery.
    if let Some(hmcl_java_home) = get_env_path(w!("HMCL_JAVA_HOME")) {
        if !hmcl_java_home.is_empty() {
            log_fmt(format_args!(
                "HMCL_JAVA_HOME: {}",
                WideDisplay(hmcl_java_home.as_slice())
            ));

            if let Some(mut java_executable) = hmcl_java_home.try_clone() {
                if java_executable.push_path_component_str("bin")
                    && java_executable.push_path_component_str(java_executable_name)
                {
                    if is_regular_file(&java_executable) {
                        if launch_jvm(&java_executable, &options) {
                            return 0;
                        }
                    } else {
                        log_fmt(format_args!(
                            "Invalid HMCL_JAVA_HOME: {}",
                            WideDisplay(hmcl_java_home.as_slice())
                        ));
                    }
                }
            } else {
                log_fmt(format_args!(
                    "Invalid HMCL_JAVA_HOME: {}",
                    WideDisplay(hmcl_java_home.as_slice())
                ));
            }

            unsafe {
                MessageBoxW(
                    ptr::null_mut(),
                    i18n.error_invalid_hmcl_java_home,
                    ptr::null(),
                    MB_OK | MB_ICONERROR,
                );
            }
            return 1;
        } else {
            log_verbose_fmt(format_args!("HMCL_JAVA_HOME: Not Found"));
        }
    } else {
        log_verbose_fmt(format_args!("HMCL_JAVA_HOME: Not Found"));
    }

    if let Some(mut bundled_jre) = options.workdir.try_clone() {
        if bundled_jre.push_path_component_str(arch.bundled_jre_dir())
            && bundled_jre.push_path_component_str("bin")
            && bundled_jre.push_path_component_str(java_executable_name)
        {
            if crate::platform::is_regular_file(&bundled_jre) {
                log_fmt(format_args!(
                    "Bundled JRE: {}",
                    WideDisplay(bundled_jre.as_slice())
                ));
                if launch_jvm(&bundled_jre, &options) {
                    return 0;
                }
            } else {
                log_verbose_fmt(format_args!("Bundled JRE: Not Found"));
            }
        }
    }

    let java_home = get_env_path(w!("JAVA_HOME"));
    if let Some(java_home) = java_home.as_ref() {
        if !java_home.is_empty() {
            log_fmt(format_args!(
                "JAVA_HOME: {}",
                WideDisplay(java_home.as_slice())
            ));
        } else {
            log_verbose_fmt(format_args!("JAVA_HOME: Not Found"));
        }
    } else {
        log_verbose_fmt(format_args!("JAVA_HOME: Not Found"));
    }

    let mut java_runtimes = JavaList::new();

    // Prefer per-instance HMCL-managed runtimes in the launcher's working
    // directory before checking global locations.
    if let Some(mut hmcl_java_dir) = options.workdir.try_clone() {
        if hmcl_java_dir.push_path_component_str(".hmcl")
            && hmcl_java_dir.push_path_component_str("java")
            && hmcl_java_dir.push_path_component_str(arch.hmcl_java_dir())
        {
            search_java_in_dir(&mut java_runtimes, &hmcl_java_dir, java_executable_name);
        }
    }

    if let Some(java_home) = java_home.as_ref() {
        if !java_home.is_empty() {
            log_verbose_fmt(format_args!("Checking JAVA_HOME"));

            if let Some(mut java_executable) = java_home.try_clone() {
                if java_executable.push_path_component_str("bin")
                    && java_executable.push_path_component_str(java_executable_name)
                {
                    if is_regular_file(&java_executable) {
                        let _ = java_runtimes.try_add(java_executable);
                    } else {
                        log_fmt(format_args!(
                            "JAVA_HOME is set to {}, but the Java executable {} does not exist",
                            WideDisplay(java_home.as_slice()),
                            WideDisplay(java_executable.as_slice())
                        ));
                    }
                }
            }
        }
    }

    if let Some(app_data) = get_env_path(w!("APPDATA")) {
        if !app_data.is_empty() {
            if let Some(mut hmcl_java_dir) = app_data.try_clone() {
                if hmcl_java_dir.push_path_component_str(".hmcl")
                    && hmcl_java_dir.push_path_component_str("java")
                    && hmcl_java_dir.push_path_component_str(arch.hmcl_java_dir())
                {
                    search_java_in_dir(&mut java_runtimes, &hmcl_java_dir, java_executable_name);
                }
            }
        }
    }

    if let Some(path) = get_env_var(w!("PATH")) {
        log_verbose_fmt(format_args!("Searching in PATH"));
        search_java_in_path(&mut java_runtimes, path.as_slice(), java_executable_name);
    } else {
        log_fmt(format_args!("PATH: Not Found"));
    }

    let program_files = match arch {
        Arch::ARM64 | Arch::X86_64 => get_env_path(w!("ProgramW6432")),
        Arch::X86 => get_env_path(w!("ProgramFiles")),
    };
    if let Some(program_files) = program_files {
        if !program_files.is_empty() {
            search_java_in_program_files(&mut java_runtimes, &program_files, java_executable_name);
        } else {
            log_fmt(format_args!("Failed to obtain the path to Program Files"));
        }
    } else {
        log_fmt(format_args!("Failed to obtain the path to Program Files"));
    }

    search_java_in_registry(
        &mut java_runtimes,
        w!("SOFTWARE\\JavaSoft\\JDK"),
        java_executable_name,
    );
    search_java_in_registry(
        &mut java_runtimes,
        w!("SOFTWARE\\JavaSoft\\JRE"),
        java_executable_name,
    );

    if java_runtimes.runtimes.is_empty() {
        log_fmt(format_args!("No Java runtime found."));
    } else {
        java_runtimes.sort_by_version();

        // Emit the full runtime list only in verbose mode because it can be
        // fairly noisy on machines with several JDKs installed.
        if crate::debug::verbose_output() {
            let mut message = WideString::new();
            if message.push_str("Found Java runtimes:") {
                for item in java_runtimes.runtimes.iter() {
                    let _ = write!(
                        &mut message,
                        "\n  - {}, Version {}",
                        WideDisplay(item.executable_path.as_slice()),
                        item.version
                    );
                }
                crate::debug::log_wide(message.as_slice());
            }
        }

        let runtimes = java_runtimes.runtimes.as_slice();
        let mut index = runtimes.len();
        while index > 0 {
            // Try higher versions first after sorting ascending.
            index -= 1;
            if launch_jvm(&runtimes[index].executable_path, &options) {
                return 0;
            }
        }
    }

    unsafe {
        // Keep the original UX: offer to open the architecture-specific Java
        // download page only after every discovery path has failed.
        if MessageBoxW(
            ptr::null_mut(),
            i18n.error_java_not_found,
            ptr::null(),
            MB_ICONWARNING | MB_OKCANCEL,
        ) == IDOK
        {
            ShellExecuteW(
                ptr::null_mut(),
                ptr::null(),
                arch.download_link(),
                ptr::null(),
                ptr::null(),
                SW_SHOW,
            );
        }
    }
    1
}
