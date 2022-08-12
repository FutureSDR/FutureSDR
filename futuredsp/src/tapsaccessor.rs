extern crate alloc;
use alloc::vec::Vec;
use num_complex::Complex32;

pub trait TapsAccessor: Send {
    type TapType;

    fn num_taps(&self) -> usize;

    /// Gets the `index`th tap.
    ///
    /// # Safety
    /// The invariant `index < num_taps()` must be upheld.
    unsafe fn get(&self, index: usize) -> Self::TapType;
}

impl<const N: usize> TapsAccessor for [Complex32; N] {
    type TapType = Complex32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> Complex32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> TapsAccessor for &[Complex32; N] {
    type TapType = Complex32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> Complex32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> TapsAccessor for [f32; N] {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> TapsAccessor for &[f32; N] {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl TapsAccessor for Vec<f32> {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        self.len()
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}
