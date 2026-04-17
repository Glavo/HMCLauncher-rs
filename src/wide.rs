use core::char::{REPLACEMENT_CHARACTER, decode_utf16};
use core::fmt::{self, Display, Formatter, Write};
use core::mem::size_of;
use core::ptr::{self, NonNull};
use core::slice;
use windows_sys::core::{PCWSTR, w};

use crate::heap::{alloc_bytes, free_bytes, realloc_bytes};

/// Owned UTF-16 storage that always keeps one trailing NUL for Win32 APIs.
pub struct WideString {
    ptr: *mut u16,
    len: usize,
    cap: usize,
}

impl WideString {
    /// Create an empty UTF-16 buffer with no heap allocation.
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::<u16>::dangling().as_ptr(),
            len: 0,
            cap: 0,
        }
    }

    /// Build a UTF-16 string by encoding UTF-8 input.
    pub fn from_str(value: &str) -> Option<Self> {
        let mut output = Self::new();
        if output.push_str(value) {
            Some(output)
        } else {
            None
        }
    }

    /// Copy an existing UTF-16 slice into owned storage.
    pub fn from_utf16(value: &[u16]) -> Option<Self> {
        let mut output = Self::new();
        if output.push_slice(value) {
            Some(output)
        } else {
            None
        }
    }

    /// Return the length without the trailing NUL terminator.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Report whether the logical string is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow the initialized UTF-16 contents without the trailing terminator.
    pub fn as_slice(&self) -> &[u16] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    /// Expose the buffer as a Win32 `PCWSTR`.
    pub fn as_pcwstr(&self) -> PCWSTR {
        if self.len == 0 { w!("") } else { self.ptr }
    }

    /// Expose the mutable storage for Win32 APIs that fill caller-owned
    /// buffers.
    pub fn as_mut_ptr(&mut self) -> *mut u16 {
        self.ptr
    }

    /// Clone the string into a second heap-backed buffer.
    pub fn try_clone(&self) -> Option<Self> {
        Self::from_utf16(self.as_slice())
    }

    /// Compare the UTF-16 string with a UTF-8 literal without allocating.
    pub fn equals_str(&self, other: &str) -> bool {
        let mut count = 0usize;
        for _ in other.encode_utf16() {
            count += 1;
        }
        if self.len != count {
            return false;
        }

        for (left, right) in self.as_slice().iter().copied().zip(other.encode_utf16()) {
            if left != right {
                return false;
            }
        }
        true
    }

    /// Reset the logical length while preserving any existing allocation.
    pub fn clear(&mut self) {
        self.len = 0;
        if self.cap != 0 {
            unsafe {
                *self.ptr = 0;
            }
        }
    }

    /// Ensure the buffer can hold `required` code units plus one trailing NUL.
    pub fn reserve_exact(&mut self, required: usize) -> bool {
        if required <= self.cap {
            return true;
        }

        let mut new_cap = if self.cap == 0 {
            required.max(16)
        } else {
            self.cap
        };
        while new_cap < required {
            let Some(doubled) = new_cap.checked_mul(2) else {
                new_cap = required;
                break;
            };
            if doubled <= new_cap {
                new_cap = required;
                break;
            }
            new_cap = doubled;
        }

        let Some(units) = new_cap.checked_add(1) else {
            return false;
        };
        let Some(bytes) = units.checked_mul(size_of::<u16>()) else {
            return false;
        };

        let new_ptr = unsafe {
            if self.cap == 0 {
                alloc_bytes(bytes)
            } else {
                realloc_bytes(self.ptr.cast(), bytes)
            }
        } as *mut u16;

        if new_ptr.is_null() {
            return false;
        }

        self.ptr = new_ptr;
        self.cap = new_cap;
        // Preserve the Win32 invariant that the logical string is always
        // terminated, even when the buffer is reused.
        unsafe {
            *self.ptr.add(self.len) = 0;
        }
        true
    }

    /// Update the logical length after a Win32 API wrote directly into the
    /// buffer.
    pub unsafe fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.cap != 0 {
            unsafe {
                *self.ptr.add(len) = 0;
            }
        }
    }

    /// Append a single Unicode scalar value.
    pub fn push_char(&mut self, value: char) -> bool {
        let mut buffer = [0u16; 2];
        let encoded = value.encode_utf16(&mut buffer);
        self.push_slice(encoded)
    }

    /// Append raw UTF-16 code units.
    pub fn push_slice(&mut self, value: &[u16]) -> bool {
        if value.is_empty() {
            return true;
        }

        let Some(required) = self.len.checked_add(value.len()) else {
            return false;
        };
        if !self.reserve_exact(required) {
            return false;
        }

        unsafe {
            ptr::copy_nonoverlapping(value.as_ptr(), self.ptr.add(self.len), value.len());
            self.len = required;
            *self.ptr.add(self.len) = 0;
        }
        true
    }

    /// Append a UTF-8 string after encoding it to UTF-16.
    pub fn push_str(&mut self, value: &str) -> bool {
        let additional = value.encode_utf16().count();
        let Some(required) = self.len.checked_add(additional) else {
            return false;
        };
        if !self.reserve_exact(required) {
            return false;
        }

        let mut offset = self.len;
        for unit in value.encode_utf16() {
            unsafe {
                *self.ptr.add(offset) = unit;
            }
            offset += 1;
        }
        self.len = offset;
        unsafe {
            *self.ptr.add(self.len) = 0;
        }
        true
    }

    /// Append `\` when the current path does not already end with a separator.
    pub fn push_path_separator(&mut self) -> bool {
        if self.is_empty() {
            return true;
        }

        let last = self.as_slice()[self.len - 1];
        if last == b'\\' as u16 || last == b'/' as u16 {
            true
        } else {
            self.push_char('\\')
        }
    }

    /// Append one UTF-16 path component, inserting a separator if required.
    pub fn push_path_component(&mut self, value: &[u16]) -> bool {
        if value.is_empty() {
            return true;
        }
        self.push_path_separator() && self.push_slice(value)
    }

    /// Append one UTF-8 path component, inserting a separator if required.
    pub fn push_path_component_str(&mut self, value: &str) -> bool {
        if value.is_empty() {
            return true;
        }
        self.push_path_separator() && self.push_str(value)
    }
}

impl Write for WideString {
    /// Support `core::fmt::Write` so launcher code can format directly into
    /// UTF-16 buffers.
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.push_str(value) {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

impl Drop for WideString {
    /// Release the owned UTF-16 buffer.
    fn drop(&mut self) {
        if self.cap != 0 {
            unsafe {
                free_bytes(self.ptr.cast());
            }
        }
    }
}

/// Render a borrowed UTF-16 slice through Rust's formatting traits.
pub struct WideDisplay<'a>(pub &'a [u16]);

impl Display for WideDisplay<'_> {
    /// Decode UTF-16 for diagnostics, replacing invalid surrogate pairs.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for result in decode_utf16(self.0.iter().copied()) {
            formatter.write_char(result.unwrap_or(REPLACEMENT_CHARACTER))?;
        }
        Ok(())
    }
}

/// Count code units in a NUL-terminated UTF-16 string.
pub fn wide_strlen(mut value: PCWSTR) -> usize {
    if value.is_null() {
        return 0;
    }

    let mut len = 0usize;
    unsafe {
        while *value != 0 {
            len += 1;
            value = value.add(1);
        }
    }
    len
}

/// Borrow a Win32-owned NUL-terminated UTF-16 string as a Rust slice.
pub unsafe fn wide_slice_from_ptr<'a>(value: PCWSTR) -> &'a [u16] {
    let len = wide_strlen(value);
    if len == 0 {
        &[]
    } else {
        // Callers only use this for NUL-terminated buffers owned by Win32.
        unsafe { slice::from_raw_parts(value, len) }
    }
}

/// Search for one UTF-16 slice inside another.
pub fn wide_contains(haystack: &[u16], needle: &[u16]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Trim leading and trailing Unicode whitespace from a UTF-16 slice.
pub fn trim_wide_whitespace(value: &[u16]) -> &[u16] {
    let mut start = 0usize;
    let mut end = value.len();

    while start < end && wide_is_whitespace(value[start]) {
        start += 1;
    }

    while end > start && wide_is_whitespace(value[end - 1]) {
        end -= 1;
    }

    &value[start..end]
}

/// Match the pseudo-directory names that must be skipped during enumeration.
pub fn is_dot_or_dot_dot(value: &[u16]) -> bool {
    value == [b'.' as u16] || value == [b'.' as u16, b'.' as u16]
}

/// Treat a UTF-16 code unit as whitespace when it maps to a scalar value.
fn wide_is_whitespace(value: u16) -> bool {
    core::char::from_u32(value as u32)
        .map(|ch| ch.is_whitespace())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{WideString, trim_wide_whitespace, wide_contains};

    #[test]
    /// Trim surrounding whitespace while keeping the inner contents unchanged.
    fn trim_spaces() {
        let value = WideString::from_str("  hello\t").unwrap();
        let trimmed = trim_wide_whitespace(value.as_slice());
        assert_eq!(
            trimmed,
            "hello".encode_utf16().collect::<std::vec::Vec<_>>()
        );
    }

    #[test]
    /// Find a path fragment inside a UTF-16 path string.
    fn substring_search() {
        let value = WideString::from_str("C:\\Common Files\\Oracle\\Java\\bin").unwrap();
        let needle = WideString::from_str("\\Common Files\\Oracle\\Java\\").unwrap();
        assert!(wide_contains(value.as_slice(), needle.as_slice()));
    }
}
