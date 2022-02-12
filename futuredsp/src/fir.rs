#[cfg(not(RUSTC_IS_STABLE))]
use std::intrinsics::{fadd_fast, fmul_fast};

use num_complex::Complex;

/// Implements a trait to run computations with FIR filters.
pub trait FirKernel<SampleType>: Send {
    /// Computes the FIR filter on the given input, outputting into the given output.
    /// Note that filters will not generally have internal memory - therefore, even
    /// if the output is sufficiently large, not all input samples may be consumed.
    /// However, it is also permitted for kernel implementations to contain state
    /// related to the input stream (for example, it may contain an internal buffer
    /// of the last `num_taps` input samples).
    ///
    /// Returns a tuple containing, in order, the samples consumed from the input
    /// and the samples produced in the output. Elements of `output` beyond what
    /// is produced are left in an unspecified state.
    fn work(&self, input: &[SampleType], output: &mut [SampleType]) -> (usize, usize);

    /// Returns the number of taps this FIR filter represents.
    fn num_taps(&self) -> usize;
}

pub trait TapsAccessor: Send {
    type TapType;

    fn num_taps(&self) -> usize;

    /// Gets the `index`th tap.
    ///
    /// Safety: The invariant `index < num_taps()` must be upheld.
    unsafe fn get(&self, index: usize) -> Self::TapType;
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

/// A non-resampling FIR filter. Calling `work()` on this struct always
/// produces exactly as many samples as it consumes.
///
/// Implementations of this core exist for the following combinations:
/// - `f32` samples, `f32` taps.
/// - `Complex<f32>` samples, `f32` taps.
///
/// Example usage:
/// ```
/// use futuredsp::fir::{FirKernel, NonResamplingFirKernel};
///
/// let fir = NonResamplingFirKernel::<f32, _>::new([1.0, 2.0, 3.0]);
///
/// let input = [1.0, 2.0, 3.0];
/// let mut output = [0.0];
/// fir.work(&input, &mut output);
/// ```
pub struct NonResamplingFirKernel<SampleType, TapsType: TapsAccessor> {
    taps: TapsType,
    _sampletype: std::marker::PhantomData<SampleType>,
}

impl<SampleType, TapsType: TapsAccessor> NonResamplingFirKernel<SampleType, TapsType> {
    /// Create a new non-resampling FIR filter using the given taps.
    pub fn new(taps: TapsType) -> Self {
        Self {
            taps,
            _sampletype: std::marker::PhantomData,
        }
    }
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<f32>
    for NonResamplingFirKernel<f32, TapsType>
{
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum = fadd_fast(sum, fmul_fast(*i.get_unchecked(k + t), self.taps.get(t)));
                }
                *o.get_unchecked_mut(k) = sum;
            }
        }

        (n, n)
    }

    fn num_taps(&self) -> usize {
        self.taps.num_taps()
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<f32>
    for NonResamplingFirKernel<f32, TapsType>
{
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum += i.get_unchecked(k + t) * self.taps.get(t);
                }
                *o.get_unchecked_mut(k) = sum;
            }
        }

        (n, n)
    }

    fn num_taps(&self) -> usize {
        self.taps.num_taps()
    }
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<Complex<f32>>
    for NonResamplingFirKernel<Complex<f32>, TapsType>
{
    fn work(&self, i: &[Complex<f32>], o: &mut [Complex<f32>]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum_re = 0.0;
                let mut sum_im = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum_re = fadd_fast(
                        sum_re,
                        fmul_fast(i.get_unchecked(k + t).re, self.taps.get(t)),
                    );
                    sum_im = fadd_fast(
                        sum_im,
                        fmul_fast(i.get_unchecked(k + t).im, self.taps.get(t)),
                    );
                }
                *o.get_unchecked_mut(k) = Complex {
                    re: sum_re,
                    im: sum_im,
                };
            }
        }

        (n, n)
    }

    fn num_taps(&self) -> usize {
        self.taps.num_taps()
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<Complex<f32>>
    for NonResamplingFirKernel<Complex<f32>, TapsType>
{
    fn work(&self, i: &[Complex<f32>], o: &mut [Complex<f32>]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum_re = 0.0;
                let mut sum_im = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum_re += i.get_unchecked(k + t).re * self.taps.get(t);
                    sum_im += i.get_unchecked(k + t).im * self.taps.get(t);
                }
                *o.get_unchecked_mut(k) = Complex {
                    re: sum_re,
                    im: sum_im,
                };
            }
        }

        (n, n)
    }

    fn num_taps(&self) -> usize {
        self.taps.num_taps()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_fir_kernel() {
        let taps = [1.0, 2.0, 3.0];
        let kernel = NonResamplingFirKernel::new(taps);
        let input = [1.0, 2.0, 3.0];
        let mut output = [0.0; 3];
        assert_eq!(kernel.work(&input, &mut output), (1, 1));
        assert_eq!(output[0], 14.0);

        let mut output = [];
        assert_eq!(kernel.work(&input, &mut output), (0, 0));

        let mut output = [0.0; 3];
        assert_eq!(kernel.work(&input, &mut output), (1, 1));
        assert_eq!(output[0], 14.0);

        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 2];
        assert_eq!(kernel.work(&input, &mut output), (2, 2));
        assert_eq!(output[0], 14.0);
        assert_eq!(output[1], 20.0);
    }
}
