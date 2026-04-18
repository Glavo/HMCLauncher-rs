use windows_sys::core::PCWSTR;

use crate::wide::WideString;

/// Owned UTF-16 storage for filesystem paths.
pub struct WidePathBuf {
    inner: WideString,
}

impl WidePathBuf {
    /// Create an empty path buffer with no heap allocation.
    pub const fn new() -> Self {
        Self {
            inner: WideString::new(),
        }
    }

    /// Build a UTF-16 path by encoding UTF-8 input.
    pub fn from_str(value: &str) -> Option<Self> {
        Some(Self {
            inner: WideString::from_str(value)?,
        })
    }

    /// Copy an existing UTF-16 path into owned storage.
    pub fn from_utf16(value: &[u16]) -> Option<Self> {
        Some(Self {
            inner: WideString::from_utf16(value)?,
        })
    }

    /// Wrap an existing UTF-16 string whose contents represent a path.
    pub fn from_wide_string(value: WideString) -> Self {
        Self { inner: value }
    }

    /// Report whether the path is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Borrow the initialized UTF-16 contents without the trailing terminator.
    pub fn as_slice(&self) -> &[u16] {
        self.inner.as_slice()
    }

    /// Expose the path buffer as a Win32 `PCWSTR`.
    pub fn as_pcwstr(&self) -> PCWSTR {
        self.inner.as_pcwstr()
    }

    /// Clone the path into a second heap-backed buffer.
    pub fn try_clone(&self) -> Option<Self> {
        Some(Self {
            inner: self.inner.try_clone()?,
        })
    }

    /// Append `\` when the current path does not already end with a separator.
    pub fn push_path_separator(&mut self) -> bool {
        let Some(&last) = self.inner.as_slice().last() else {
            return true;
        };

        if last == b'\\' as u16 || last == b'/' as u16 {
            true
        } else {
            self.inner.push_char('\\')
        }
    }

    /// Append one UTF-16 path component, inserting a separator if required.
    pub fn push_path_component(&mut self, value: &[u16]) -> bool {
        if value.is_empty() {
            return true;
        }
        self.push_path_separator() && self.inner.push_slice(value)
    }

    /// Append one UTF-8 path component, inserting a separator if required.
    pub fn push_path_component_str(&mut self, value: &str) -> bool {
        if value.is_empty() {
            return true;
        }
        self.push_path_separator() && self.inner.push_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::WidePathBuf;
    use crate::wide::wide_contains;

    /// Find a path fragment inside a UTF-16 path string.
    #[test]
    fn substring_search() {
        let value = WidePathBuf::from_str("C:\\Common Files\\Oracle\\Java\\bin").unwrap();
        let needle = WidePathBuf::from_str("\\Common Files\\Oracle\\Java\\").unwrap();
        assert!(wide_contains(value.as_slice(), needle.as_slice()));
    }

    /// Join path components without duplicating separators.
    #[test]
    fn push_path_component() {
        let mut value = WidePathBuf::from_str("C:\\Java").unwrap();
        assert!(value.push_path_component_str("bin"));
        assert_eq!(
            value.as_slice(),
            "C:\\Java\\bin".encode_utf16().collect::<std::vec::Vec<_>>()
        );
    }
}
