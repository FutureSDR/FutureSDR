use std::alloc::Layout;
use std::alloc::alloc;
use std::alloc::dealloc;
use std::alloc::handle_alloc_error;
use std::fmt;
use std::mem::size_of;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::NonNull;

use crate::runtime::buffer::CpuSample;

const CACHE_LINE_BYTES: usize = 64;

/// Cache-line-aligned CPU sample storage.
#[doc(hidden)]
pub struct CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    ptr: NonNull<T>,
    len: usize,
}

impl<T> CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    /// Allocate and default-initialize `len` samples.
    pub fn new(len: usize) -> Self {
        assert_ne!(
            size_of::<T>(),
            0,
            "cache-aligned buffer items cannot be zero-sized"
        );
        if len == 0 {
            return Self {
                ptr: NonNull::dangling(),
                len,
            };
        }

        let value = T::default();
        let layout = Self::layout(len);
        // SAFETY: `layout` has non-zero size because both `T` and `len` are
        // non-zero. The returned allocation is checked before it is used.
        let ptr = unsafe { alloc(layout).cast::<T>() };
        let Some(ptr) = NonNull::new(ptr) else {
            handle_alloc_error(layout);
        };

        for index in 0..len {
            // SAFETY: `layout` contains `len` properly aligned `T` slots and
            // every index in this loop is initialized exactly once.
            unsafe {
                ptr.add(index).write(value);
            }
        }

        Self { ptr, len }
    }

    fn layout(len: usize) -> Layout {
        Layout::array::<T>(len)
            .and_then(|layout| layout.align_to(CACHE_LINE_BYTES))
            .expect("cache-aligned buffer allocation is too large")
    }
}

impl<T> Deref for CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        // SAFETY: `ptr` addresses `len` initialized samples, or is a properly
        // aligned dangling pointer when `len` is zero.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: The buffer owns the allocation and `&mut self` guarantees
        // exclusive access to all `len` initialized samples.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> fmt::Debug for CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Drop for CacheAlignedBuffer<T>
where
    T: CpuSample,
{
    fn drop(&mut self) {
        if self.len == 0 {
            return;
        }

        // SAFETY: This is the allocation created by `new` with the same
        // layout. All `len` elements are initialized and exclusively owned.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.ptr.as_ptr(),
                self.len,
            ));
            dealloc(self.ptr.as_ptr().cast(), Self::layout(self.len));
        }
    }
}

// SAFETY: The allocation is exclusively owned by the buffer and `T` is Send.
unsafe impl<T> Send for CacheAlignedBuffer<T> where T: CpuSample {}

// SAFETY: Shared access only exposes shared references and `T` is Sync.
unsafe impl<T> Sync for CacheAlignedBuffer<T> where T: CpuSample {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_is_cache_line_aligned_and_initialized() {
        let buffer = CacheAlignedBuffer::<u32>::new(31);

        assert_eq!(buffer.as_ptr().addr() % CACHE_LINE_BYTES, 0);
        assert!(buffer.iter().all(|item| *item == 0));
    }

    #[test]
    fn empty_allocation_is_supported() {
        let buffer = CacheAlignedBuffer::<u8>::new(0);

        assert!(buffer.is_empty());
    }

    #[test]
    #[should_panic(expected = "cache-aligned buffer items cannot be zero-sized")]
    fn zero_sized_items_are_rejected() {
        let _ = CacheAlignedBuffer::<()>::new(1);
    }

    #[repr(align(128))]
    #[derive(Clone, Copy, Debug, Default)]
    struct OverAligned(u8);

    #[test]
    fn preserves_larger_item_alignment() {
        let buffer = CacheAlignedBuffer::<OverAligned>::new(2);

        assert_eq!(buffer.as_ptr().addr() % align_of::<OverAligned>(), 0);
        assert_eq!(buffer[0].0, 0);
    }
}
