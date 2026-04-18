use core::char::{REPLACEMENT_CHARACTER, decode_utf16};
use core::fmt::{self, Display, Formatter, Write};
use core::ptr::{self, NonNull};
use core::slice;
use windows_sys::core::{PCWSTR, w};

/// Owned UTF-16 storage that always keeps one trailing NUL for Win32 APIs.
pub struct WideString {
    data: Vec<u16>,
    len: usize,
}

impl WideString {
    /// Create an empty UTF-16 buffer with no heap allocation.
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            len: 0,
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
            &self.data[..self.len]
        }
    }

    /// Expose the buffer as a Win32 `PCWSTR`.
    pub fn as_pcwstr(&self) -> PCWSTR {
        if self.len == 0 {
            w!("")
        } else {
            self.data.as_ptr()
        }
    }

    /// Expose the mutable storage for Win32 APIs that fill caller-owned
    /// buffers.
    pub fn as_mut_ptr(&mut self) -> *mut u16 {
        if self.data.is_empty() {
            NonNull::<u16>::dangling().as_ptr()
        } else {
            self.data.as_mut_ptr()
        }
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
        if !self.data.is_empty() {
            self.data[0] = 0;
        }
    }

    /// Ensure the buffer can hold `required` code units plus one trailing NUL.
    pub fn reserve_exact(&mut self, required: usize) -> bool {
        let Some(units) = required.checked_add(1) else {
            return false;
        };

        if self.data.len() < units {
            self.data.resize(units, 0);
        } else if !self.data.is_empty() {
            self.data[self.len] = 0;
        }
        true
    }

    /// Update the logical length after a Win32 API wrote directly into the
    /// buffer.
    pub unsafe fn set_len(&mut self, len: usize) {
        if self.data.len() <= len {
            self.data.resize(len + 1, 0);
        }
        self.len = len;
        self.data[len] = 0;
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
            ptr::copy_nonoverlapping(
                value.as_ptr(),
                self.data.as_mut_ptr().add(self.len),
                value.len(),
            );
            self.len = required;
            *self.data.as_mut_ptr().add(self.len) = 0;
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
            self.data[offset] = unit;
            offset += 1;
        }
        self.len = offset;
        self.data[self.len] = 0;
        true
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
    use super::{WideString, trim_wide_whitespace};

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
}
