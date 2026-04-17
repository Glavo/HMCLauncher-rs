/*
 * Copyright (C) 2025 Glavo. All rights reserved.
 * DO NOT ALTER OR REMOVE COPYRIGHT NOTICES OR THIS FILE HEADER.
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use crate::platform::Arch::X86;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation;
use windows::Win32::System::SystemInformation::{
    GetNativeSystemInfo, PROCESSOR_ARCHITECTURE_ARM64, SYSTEM_INFO,
};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::core::imp::GetProcAddress;
use windows::core::w;

pub enum Arch {
    X86,
    X86_64,
    ARM64,
}

pub static ARCH: Arch = Arch::get();

impl Arch {
    pub fn get() -> Arch {
        unsafe {
            if let Ok(kernel32) = GetModuleHandleW(w!("Kernel32.dll"))
                && !kernel32.is_invalid()
            {
                let is_wow64process2 = GetProcAddress(kernel32.0, "IsWow64Process2\0".as_ptr());

                if let Some(addr) = is_wow64process2 {
                    type IsWow64Process2Fn =
                    unsafe extern "system" fn(HANDLE, *mut u16, *mut u16) -> i32;

                    let mut u_process_machine = 0u16;
                    let mut u_native_machine = 0u16;

                    let func: IsWow64Process2Fn = std::mem::transmute(addr);
                    func(
                        GetCurrentProcess(),
                        &mut u_process_machine,
                        &mut u_native_machine,
                    );

                    return match u_native_machine {
                        0x8664u16 => Arch::X86_64, // IMAGE_FILE_MACHINE_AMD64
                        0xAA64u16 => Arch::ARM64,  // IMAGE_FILE_MACHINE_ARM64
                        _ => Arch::X86,
                    };
                }
            }

            let mut system_info = SYSTEM_INFO::default();
            GetNativeSystemInfo(&mut system_info);
            match system_info.Anonymous.Anonymous.wProcessorArchitecture.0 {
                9u16 => Arch::X86_64,  // PROCESSOR_ARCHITECTURE_AMD64
                12u16 => Arch::X86_64, // PROCESSOR_ARCHITECTURE_ARM64
                _ => X86,
            }
        }
    }
}
