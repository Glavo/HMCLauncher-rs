use core::mem::zeroed;
use core::ptr;
use windows_sys::Win32::Foundation::{
    ERROR_SUCCESS, GetLastError, HMODULE, MAX_PATH, SetLastError,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW,
    INVALID_FILE_ATTRIBUTES,
};
use windows_sys::Win32::System::Environment::GetEnvironmentVariableW;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleW, GetProcAddress,
};
use windows_sys::Win32::System::SystemInformation::{
    GetNativeSystemInfo, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64, SYSTEM_INFO,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::core::{PCWSTR, s, w};

use crate::wide::WideString;

pub struct SelfPath {
    pub workdir: WideString,
    pub jar_path: WideString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86,
    X86_64,
    ARM64,
}

impl Arch {
    pub fn current() -> Self {
        unsafe {
            let kernel32: HMODULE = GetModuleHandleW(w!("Kernel32.dll"));
            if !kernel32.is_null() {
                let proc = GetProcAddress(kernel32, s!("IsWow64Process2"));
                if let Some(raw_proc) = proc {
                    type IsWow64Process2Fn = unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u16,
                        *mut u16,
                    ) -> i32;

                    let mut process_machine = 0u16;
                    let mut native_machine = 0u16;
                    let func: IsWow64Process2Fn = core::mem::transmute(raw_proc);
                    if func(
                        GetCurrentProcess(),
                        &mut process_machine,
                        &mut native_machine,
                    ) != 0
                    {
                        return match native_machine {
                            IMAGE_FILE_MACHINE_ARM64 => Self::ARM64,
                            IMAGE_FILE_MACHINE_AMD64 => Self::X86_64,
                            _ => Self::X86,
                        };
                    }
                }
            }

            let mut system_info: SYSTEM_INFO = zeroed();
            GetNativeSystemInfo(&mut system_info);
            match system_info.Anonymous.Anonymous.wProcessorArchitecture {
                PROCESSOR_ARCHITECTURE_ARM64 => Self::ARM64,
                PROCESSOR_ARCHITECTURE_AMD64 => Self::X86_64,
                _ => Self::X86,
            }
        }
    }

    pub fn bundled_jre_dir(self) -> &'static str {
        match self {
            Self::ARM64 => "jre-arm64",
            Self::X86_64 => "jre-x64",
            Self::X86 => "jre-x86",
        }
    }

    pub fn hmcl_java_dir(self) -> &'static str {
        match self {
            Self::ARM64 => "windows-arm64",
            Self::X86_64 => "windows-x86_64",
            Self::X86 => "windows-x86",
        }
    }

    pub fn download_link(self) -> PCWSTR {
        match self {
            Self::ARM64 => w!("https://docs.hmcl.net/downloads/windows/arm64.html"),
            Self::X86_64 => w!("https://docs.hmcl.net/downloads/windows/x86_64.html"),
            Self::X86 => w!("https://docs.hmcl.net/downloads/windows/x86.html"),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ARM64 => "arm64",
            Self::X86_64 => "x86-64",
            Self::X86 => "x86",
        }
    }
}

pub fn get_self_path() -> Option<SelfPath> {
    let mut size = MAX_PATH as usize;
    let mut buffer = WideString::new();

    loop {
        if !buffer.reserve_exact(size) {
            return None;
        }

        let result =
            unsafe { GetModuleFileNameW(ptr::null_mut(), buffer.as_mut_ptr(), size as u32) }
                as usize;
        if result == 0 {
            return None;
        }

        if result < size {
            unsafe {
                buffer.set_len(result);
            }
            break;
        }

        size = size.checked_add(MAX_PATH as usize)?;
    }

    let path = buffer.as_slice();
    let slash = path
        .iter()
        .rposition(|&unit| unit == b'\\' as u16 || unit == b'/' as u16)?;
    if slash + 1 >= path.len() {
        return None;
    }

    Some(SelfPath {
        workdir: WideString::from_utf16(&path[..slash])?,
        jar_path: WideString::from_utf16(&path[slash + 1..])?,
    })
}

pub fn get_env_var(name: PCWSTR) -> Option<WideString> {
    let mut size = MAX_PATH as usize;
    let mut output = WideString::new();

    while size < 32 * 1024 {
        if !output.reserve_exact(size) {
            return None;
        }

        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let result = unsafe { GetEnvironmentVariableW(name, output.as_mut_ptr(), size as u32) };

        if result == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_SUCCESS {
                return None;
            }
        }

        let result = result as usize;
        if result < size {
            unsafe {
                output.set_len(result);
            }
            return Some(output);
        }

        size = if result == size { result + 1 } else { result };
    }

    None
}

pub fn get_env_path(name: PCWSTR) -> Option<WideString> {
    get_env_var(name)
}

pub fn is_regular_file(path: &WideString) -> bool {
    let attributes = unsafe { GetFileAttributesW(path.as_pcwstr()) };
    attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0
        && (attributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0
}
