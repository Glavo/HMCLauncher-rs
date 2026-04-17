use core::cmp::Ordering;
use core::mem::{needs_drop, size_of};
use core::ptr::{self, NonNull};
use core::slice;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapAlloc, HeapFree, HeapReAlloc};

#[inline]
/// Return the Win32 process heap used for all dynamic launcher storage.
fn process_heap() -> HANDLE {
    unsafe { GetProcessHeap() }
}

/// Allocate raw bytes from the process heap, never requesting a zero-sized
/// block from Win32.
pub unsafe fn alloc_bytes(bytes: usize) -> *mut u8 {
    unsafe { HeapAlloc(process_heap(), 0, bytes.max(1)) as *mut u8 }
}

/// Grow or create a raw heap block while keeping the same Win32 allocation
/// source as the rest of the launcher.
pub unsafe fn realloc_bytes(ptr: *mut u8, bytes: usize) -> *mut u8 {
    if ptr.is_null() {
        unsafe { alloc_bytes(bytes) }
    } else {
        unsafe { HeapReAlloc(process_heap(), 0, ptr.cast(), bytes.max(1)) as *mut u8 }
    }
}

/// Free a raw heap block if it was previously allocated.
pub unsafe fn free_bytes(ptr: *mut u8) {
    if !ptr.is_null() {
        unsafe {
            HeapFree(process_heap(), 0, ptr.cast());
        }
    }
}

/// A minimal Vec-like container backed by the process heap so the launcher
/// stays `no_std` and does not rely on a global allocator.
pub struct HeapVec<T> {
    ptr: *mut T,
    len: usize,
    cap: usize,
}

impl<T> HeapVec<T> {
    /// Create an empty vector backed by the process heap.
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::<T>::dangling().as_ptr(),
            len: 0,
            cap: if size_of::<T>() == 0 { usize::MAX } else { 0 },
        }
    }

    /// Report whether the vector contains any initialized elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow the initialized prefix as an immutable slice.
    pub fn as_slice(&self) -> &[T] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    /// Borrow the initialized prefix as a mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
        }
    }

    /// Iterate over the initialized elements.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Append one element, growing the backing storage if needed.
    pub fn push(&mut self, value: T) -> bool {
        if size_of::<T>() == 0 {
            self.len += 1;
            core::mem::forget(value);
            return true;
        }

        if !self.reserve(1) {
            return false;
        }

        unsafe {
            ptr::write(self.ptr.add(self.len), value);
        }
        self.len += 1;
        true
    }

    /// Reserve space for more elements using a simple growth strategy.
    pub fn reserve(&mut self, additional: usize) -> bool {
        if size_of::<T>() == 0 {
            return true;
        }

        let Some(required) = self.len.checked_add(additional) else {
            return false;
        };

        if required <= self.cap {
            return true;
        }

        // A simple doubling strategy is enough here because launcher-side
        // collections stay small and short-lived.
        let mut new_cap = if self.cap == 0 {
            required.max(4)
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

        let Some(bytes) = size_of::<T>().checked_mul(new_cap) else {
            return false;
        };

        let new_ptr = unsafe {
            if self.cap == 0 {
                alloc_bytes(bytes)
            } else {
                realloc_bytes(self.ptr.cast(), bytes)
            }
        } as *mut T;

        if new_ptr.is_null() {
            return false;
        }

        self.ptr = new_ptr;
        self.cap = new_cap;
        true
    }

    /// Sort the initialized prefix in-place with insertion sort to keep the
    /// implementation allocator-free and tiny.
    pub fn sort_by<F>(&mut self, mut compare: F)
    where
        F: FnMut(&T, &T) -> Ordering,
    {
        let slice = self.as_mut_slice();
        for index in 1..slice.len() {
            let mut current = index;
            while current > 0 && compare(&slice[current - 1], &slice[current]) == Ordering::Greater
            {
                slice.swap(current - 1, current);
                current -= 1;
            }
        }
    }
}

impl<T> Drop for HeapVec<T> {
    /// Drop initialized elements and release the backing heap allocation.
    fn drop(&mut self) {
        if needs_drop::<T>() {
            for index in 0..self.len {
                unsafe {
                    ptr::drop_in_place(self.ptr.add(index));
                }
            }
        }

        if size_of::<T>() != 0 && self.cap != 0 {
            unsafe {
                free_bytes(self.ptr.cast());
            }
        }
    }
}
