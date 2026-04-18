use core::fmt::{self, Display, Formatter};
use core::mem::{size_of, zeroed};
use core::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FindClose, FindFirstFileW, FindNextFileW, GetFileVersionInfoSizeW, GetFileVersionInfoW,
    VS_FIXEDFILEINFO, VerQueryValueW, WIN32_FIND_DATAW,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_VALUE_TYPE, RRF_RT_REG_SZ,
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, NORMAL_PRIORITY_CLASS, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows_sys::core::{PCWSTR, w};

use crate::HMCL_EXPECTED_JAVA_MAJOR_VERSION;
use crate::debug::{log_fmt, log_verbose_fmt};
use crate::platform::is_regular_file;
use crate::wide::{
    WideDisplay, WideString, is_dot_or_dot_dot, trim_wide_whitespace, wide_contains,
    wide_slice_from_ptr,
};
use crate::wide_path::WidePathBuf;

/// Collect the filesystem and JVM options needed to launch HMCL.
pub struct JavaOptions {
    pub workdir: WidePathBuf,
    pub jar_path: WidePathBuf,
    pub jvm_options: Option<WideString>,
}

/// Represent a four-part Java executable version extracted from PE metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct JavaVersion {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub revision: u16,
}

impl JavaVersion {
    /// Return the sentinel used when a Java version cannot be determined.
    pub fn invalid() -> Self {
        Self::default()
    }

    /// Read the PE version resource from a Java executable.
    pub fn from_executable(path: &WidePathBuf) -> Self {
        // Match the C++ launcher and read the executable's version resource
        // instead of spawning `java -version`.
        let size = unsafe { GetFileVersionInfoSizeW(path.as_pcwstr(), ptr::null_mut()) };
        if size == 0 {
            return Self::invalid();
        }

        let mut data = vec![0u8; size as usize];
        let result =
            unsafe { GetFileVersionInfoW(path.as_pcwstr(), 0, size, data.as_mut_ptr().cast()) };
        if result == 0 {
            return Self::invalid();
        }

        // Query the root `VS_FIXEDFILEINFO` block to extract the four numeric
        // version components in the same format as the upstream launcher.
        let mut info_ptr = ptr::null_mut();
        let mut info_len = 0u32;
        let result =
            unsafe { VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut info_ptr, &mut info_len) };
        if result == 0 || info_ptr.is_null() || info_len < size_of::<VS_FIXEDFILEINFO>() as u32 {
            return Self::invalid();
        }

        let info = unsafe { &*(info_ptr as *const VS_FIXEDFILEINFO) };
        Self {
            major: ((info.dwFileVersionMS >> 16) & 0xFFFF) as u16,
            minor: (info.dwFileVersionMS & 0xFFFF) as u16,
            build: ((info.dwFileVersionLS >> 16) & 0xFFFF) as u16,
            revision: (info.dwFileVersionLS & 0xFFFF) as u16,
        }
    }

    #[cfg(test)]
    /// Parse a UTF-16 Java version string for unit tests.
    pub fn from_utf16(value: &[u16]) -> Self {
        let mut parts = [0u16; 4];
        let mut index = 0usize;

        for unit in value.iter().copied() {
            if index >= parts.len() {
                break;
            }

            match unit {
                46 | 95 => {
                    if index == 0 && parts[0] == 1 {
                        parts[0] = 0;
                    } else {
                        index += 1;
                    }
                }
                48..=57 => {
                    parts[index] = parts[index]
                        .saturating_mul(10)
                        .saturating_add((unit - 48) as u16);
                }
                _ => {}
            }
        }

        Self {
            major: parts[0],
            minor: parts[1],
            build: parts[2],
            revision: parts[3],
        }
    }

    /// Check whether the discovered runtime meets HMCL's minimum Java level.
    pub fn is_acceptable(self) -> bool {
        self.major >= HMCL_EXPECTED_JAVA_MAJOR_VERSION
    }
}

impl Display for JavaVersion {
    /// Format the Java version for logs.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if self.major == 0 {
            formatter.write_str("Unknown")
        } else {
            write!(
                formatter,
                "{}.{}.{}.{}",
                self.major, self.minor, self.build, self.revision
            )
        }
    }
}

/// Pair a Java executable path with the version discovered from its metadata.
pub struct JavaRuntime {
    pub version: JavaVersion,
    pub executable_path: WidePathBuf,
}

/// Own the set of discovered Java runtimes before launch selection.
pub struct JavaList {
    pub runtimes: Vec<JavaRuntime>,
}

impl JavaList {
    /// Create an empty runtime list.
    pub fn new() -> Self {
        Self {
            runtimes: Vec::new(),
        }
    }

    /// Add one candidate runtime when it exists, is unique, and is new enough.
    pub fn try_add(&mut self, java_executable: WidePathBuf) -> bool {
        if !is_regular_file(&java_executable) {
            return false;
        }

        if self
            .runtimes
            .iter()
            .any(|item| item.executable_path.as_slice() == java_executable.as_slice())
        {
            log_verbose_fmt(format_args!(
                "Ignore duplicate Java {}",
                WideDisplay(java_executable.as_slice())
            ));
            return false;
        }

        let version = JavaVersion::from_executable(&java_executable);
        let ignored = if version.is_acceptable() {
            ""
        } else {
            ", Ignored"
        };
        log_verbose_fmt(format_args!(
            "Found Java {}, Version {}{}",
            WideDisplay(java_executable.as_slice()),
            version,
            ignored
        ));

        if !version.is_acceptable() {
            return false;
        }

        self.runtimes.push(JavaRuntime {
            version,
            executable_path: java_executable,
        });
        true
    }

    /// Sort runtimes from lowest to highest version so callers can try the best
    /// match last-to-first.
    pub fn sort_by_version(&mut self) {
        self.runtimes
            .sort_by(|left, right| left.version.cmp(&right.version));
    }
}

/// Launch HMCL with a specific Java executable.
pub fn launch_jvm(java_executable_path: &WidePathBuf, options: &JavaOptions) -> bool {
    let mut command = WideString::new();
    if !command.push_char('"')
        || !command.push_slice(java_executable_path.as_slice())
        || !command.push_char('"')
    {
        return false;
    }

    match options.jvm_options.as_ref() {
        Some(jvm_options) => {
            if !command.push_char(' ') || !command.push_slice(jvm_options.as_slice()) {
                return false;
            }
        }
        None => {
            if !command.push_str(" -Xmx1G -XX:MinHeapFreeRatio=5 -XX:MaxHeapFreeRatio=15") {
                return false;
            }
        }
    }

    // Preserve the upstream behavior exactly, including using the current exe
    // file name as the `-jar` target.
    if !command.push_str(" -jar \"")
        || !command.push_slice(options.jar_path.as_slice())
        || !command.push_char('"')
    {
        return false;
    }

    let mut startup_info: STARTUPINFOW = unsafe { zeroed() };
    startup_info.cb = size_of::<STARTUPINFOW>() as u32;

    let mut process_info: PROCESS_INFORMATION = unsafe { zeroed() };
    // Let CreateProcessW mutate the command line in-place, which is why the
    // buffer is owned and writable.
    let result = unsafe {
        CreateProcessW(
            ptr::null(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            NORMAL_PRIORITY_CLASS,
            ptr::null(),
            options.workdir.as_pcwstr(),
            &startup_info,
            &mut process_info,
        )
    };

    if result != 0 {
        log_fmt(format_args!(
            "Successfully launched HMCL with {}",
            WideDisplay(java_executable_path.as_slice())
        ));
        unsafe {
            CloseHandle(process_info.hProcess);
            CloseHandle(process_info.hThread);
        }
        true
    } else {
        log_fmt(format_args!(
            "Failed to launch HMCL with {}",
            WideDisplay(java_executable_path.as_slice())
        ));
        false
    }
}

/// Search one directory for subdirectories that look like Java homes.
pub fn search_java_in_dir(
    result: &mut JavaList,
    basedir: &WidePathBuf,
    java_executable_name: &str,
) {
    log_verbose_fmt(format_args!(
        "Searching in directory: {}",
        WideDisplay(basedir.as_slice())
    ));

    let Some(mut pattern) = basedir.try_clone() else {
        return;
    };
    if !pattern.push_path_component_str("*") {
        return;
    }

    let mut data: WIN32_FIND_DATAW = unsafe { zeroed() };
    let handle = unsafe { FindFirstFileW(pattern.as_pcwstr(), &mut data) };
    if handle == INVALID_HANDLE_VALUE {
        return;
    }

    loop {
        // Treat each child as a potential Java home and append `bin\java*.exe`.
        let name = unsafe { wide_slice_from_ptr(data.cFileName.as_ptr()) };
        if !is_dot_or_dot_dot(name) {
            if let Some(mut candidate) = basedir.try_clone() {
                if candidate.push_path_component(name)
                    && candidate.push_path_component_str("bin")
                    && candidate.push_path_component_str(java_executable_name)
                {
                    let _ = result.try_add(candidate);
                }
            }
        }

        if unsafe { FindNextFileW(handle, &mut data) } == 0 {
            break;
        }
    }

    unsafe {
        FindClose(handle);
    }
}

/// Probe the common vendor directories under Program Files.
pub fn search_java_in_program_files(
    result: &mut JavaList,
    program_files: &WidePathBuf,
    java_executable_name: &str,
) {
    // Search the same vendor folders as the upstream launcher.
    const VENDORS: [&str; 7] = [
        "Java",
        "Microsoft",
        "BellSoft",
        "Zulu",
        "Eclipse Foundation",
        "AdoptOpenJDK",
        "Semeru",
    ];

    for vendor in VENDORS {
        let Some(mut directory) = program_files.try_clone() else {
            return;
        };
        if directory.push_path_component_str(vendor) {
            search_java_in_dir(result, &directory, java_executable_name);
        }
    }
}

/// Enumerate Java installations registered under a specific JavaSoft key.
pub fn search_java_in_registry(result: &mut JavaList, sub_key: PCWSTR, java_executable_name: &str) {
    log_verbose_fmt(format_args!(
        "Searching in registry key: HKEY_LOCAL_MACHINE\\{}",
        WideDisplay(unsafe { wide_slice_from_ptr(sub_key) })
    ));

    let mut key: HKEY = ptr::null_mut();
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            sub_key,
            0,
            KEY_WOW64_64KEY | KEY_READ,
            &mut key,
        )
    } != ERROR_SUCCESS
    {
        return;
    }

    let mut sub_keys = 0u32;
    let result_code = unsafe {
        RegQueryInfoKeyW(
            key,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
            &mut sub_keys,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if result_code != ERROR_SUCCESS || sub_keys == 0 {
        unsafe {
            RegCloseKey(key);
        }
        return;
    }

    // JavaSoft version subkeys are short, so a fixed stack buffer is enough.
    const MAX_KEY_LENGTH: usize = 256;
    let mut java_version = [0u16; MAX_KEY_LENGTH];
    let mut java_home = [0u16; 260];

    for index in 0..sub_keys {
        let mut name_len = (MAX_KEY_LENGTH - 1) as u32;
        let enum_result = unsafe {
            RegEnumKeyExW(
                key,
                index,
                java_version.as_mut_ptr(),
                &mut name_len,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut::<FILETIME>(),
            )
        };
        if enum_result != ERROR_SUCCESS {
            continue;
        }
        java_version[name_len as usize] = 0;

        // Each subkey is a version label whose `JavaHome` value points at the
        // installation root.
        let mut value_len = (java_home.len() * size_of::<u16>()) as u32;
        let get_result = unsafe {
            RegGetValueW(
                key,
                java_version.as_ptr(),
                w!("JavaHome"),
                RRF_RT_REG_SZ,
                ptr::null_mut::<REG_VALUE_TYPE>(),
                java_home.as_mut_ptr().cast(),
                &mut value_len,
            )
        };
        if get_result != ERROR_SUCCESS {
            continue;
        }

        let mut units = (value_len as usize) / size_of::<u16>();
        if units > 0 && java_home[units - 1] == 0 {
            units -= 1;
        }

        if let Some(mut executable) = WidePathBuf::from_utf16(&java_home[..units]) {
            if executable.push_path_component_str("bin")
                && executable.push_path_component_str(java_executable_name)
            {
                let _ = result.try_add(executable);
            }
        }
    }

    unsafe {
        RegCloseKey(key);
    }
}

/// Parse `%PATH%` and test each directory for a directly accessible Java
/// executable.
pub fn search_java_in_path(result: &mut JavaList, path: &[u16], java_executable_name: &str) {
    let oracle_java = unsafe { wide_slice_from_ptr(w!("\\Common Files\\Oracle\\Java\\")) };
    let mut start = 0usize;

    while start < path.len() {
        let mut end = start;
        while end < path.len() && path[end] != b';' as u16 {
            end += 1;
        }

        let entry = trim_wide_whitespace(&path[start..end]);
        if !entry.is_empty() {
            if let Some(mut java_executable) = WidePathBuf::from_utf16(entry) {
                if java_executable.push_path_component_str(java_executable_name) {
                    if wide_contains(java_executable.as_slice(), oracle_java) {
                        // Keep the upstream exclusion for Oracle's shared shim
                        // directory, which often is not a real JVM install.
                        log_verbose_fmt(format_args!(
                            "Ignore Oracle Java {}",
                            WideDisplay(java_executable.as_slice())
                        ));
                    } else {
                        log_verbose_fmt(format_args!(
                            "Checking {}",
                            WideDisplay(java_executable.as_slice())
                        ));
                        let _ = result.try_add(java_executable);
                    }
                }
            }
        }

        start = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::JavaVersion;

    #[test]
    /// Parse a modern feature-release Java version string.
    fn parse_java_version() {
        let version =
            JavaVersion::from_utf16(&"17.0.15_9".encode_utf16().collect::<std::vec::Vec<_>>());
        assert_eq!(version.major, 17);
        assert_eq!(version.minor, 0);
        assert_eq!(version.build, 15);
        assert_eq!(version.revision, 9);
    }

    #[test]
    /// Normalize a legacy `1.x` Java version to its effective major release.
    fn parse_legacy_java_version() {
        let version =
            JavaVersion::from_utf16(&"1.8.0_321".encode_utf16().collect::<std::vec::Vec<_>>());
        assert_eq!(version.major, 8);
        assert_eq!(version.minor, 0);
        assert_eq!(version.build, 321);
    }
}
