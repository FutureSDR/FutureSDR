//! Filter Taps
extern crate alloc;
use alloc::vec::Vec;
use num_complex::Complex;
use num_traits::Float;

/// Abstraction over taps
pub trait Taps: Send {
    /// Tap type
    type TapType;

    /// Number of taps
    fn num_taps(&self) -> usize;

    /// Gets the `index`th tap.
    ///
    /// # Safety
    /// The invariant `index < num_taps()` must be upheld.
    unsafe fn get(&self, index: usize) -> Self::TapType;
}

impl<const N: usize, T> Taps for [Complex<T>; N]
where
    T: Float + Send + Sync + Copy,
{
    type TapType = Complex<T>;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> Complex<T> {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize, T> Taps for &[Complex<T>; N]
where
    T: Float + Send + Sync + Copy,
{
    type TapType = Complex<T>;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> Complex<T> {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> Taps for [f32; N] {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> Taps for &[f32; N] {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> Taps for [f64; N] {
    type TapType = f64;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f64 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<const N: usize> Taps for &[f64; N] {
    type TapType = f64;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f64 {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}

impl<T> Taps for Vec<T>
where
    T: Send + Sync + Copy,
{
    type TapType = T;

    fn num_taps(&self) -> usize {
        self.len()
    }

    unsafe fn get(&self, index: usize) -> T {
        debug_assert!(index < self.num_taps());
        *self.get_unchecked(index)
    }
}
