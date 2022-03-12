use core::cmp::Ordering;
#[cfg(not(RUSTC_IS_STABLE))]
use core::intrinsics::{fadd_fast, fmul_fast};

use crate::{ComputationStatus, TapsAccessor, UnaryKernel};
use num_complex::Complex;

/// A non-resampling FIR filter. Calling `work()` on this struct always
/// produces exactly as many samples as it consumes.
///
/// Implementations of this core exist for the following combinations:
/// - `f32` samples, `f32` taps.
/// - `Complex<f32>` samples, `f32` taps.
///
/// Example usage:
/// ```
/// use futuredsp::UnaryKernel;
/// use futuredsp::fir::NonResamplingFirKernel;
///
/// let fir = NonResamplingFirKernel::<f32, _>::new([1.0, 2.0, 3.0]);
///
/// let input = [1.0, 2.0, 3.0];
/// let mut output = [0.0];
/// fir.work(&input, &mut output);
/// ```
pub struct NonResamplingFirKernel<SampleType, TapsType: TapsAccessor> {
    taps: TapsType,
    _sampletype: core::marker::PhantomData<SampleType>,
}

impl<SampleType, TapsType: TapsAccessor> NonResamplingFirKernel<SampleType, TapsType> {
    /// Create a new non-resampling FIR filter using the given taps.
    pub fn new(taps: TapsType) -> Self {
        Self {
            taps,
            _sampletype: core::marker::PhantomData,
        }
    }
}

/// Internal helper function to abstract away everything but the core computation.
/// Note that this function gets heavily inlined, so there is no (runtime) performance
/// overhead.
fn fir_kernel_core<
    SampleType,
    TapsType: TapsAccessor,
    InitFn: Fn() -> SampleType,
    MacFn: Fn(SampleType, SampleType, TapsType::TapType) -> SampleType,
>(
    taps: &TapsType,
    i: &[SampleType],
    o: &mut [SampleType],
    init: InitFn,
    mac: MacFn,
) -> (usize, usize, ComputationStatus)
where
    SampleType: Copy,
    TapsType::TapType: Copy,
{
    let num_producable_samples = (i.len() + 1).saturating_sub(taps.num_taps());
    let (n, status) = match num_producable_samples.cmp(&o.len()) {
        Ordering::Greater => (o.len(), ComputationStatus::InsufficientOutput),
        Ordering::Equal => (num_producable_samples, ComputationStatus::BothSufficient),
        Ordering::Less => (num_producable_samples, ComputationStatus::InsufficientInput),
    };

    unsafe {
        for k in 0..n {
            let mut sum = init();
            for t in 0..taps.num_taps() {
                sum = mac(sum, *i.get_unchecked(k + t), taps.get(t));
            }
            *o.get_unchecked_mut(k) = sum;
        }
    }

    (n, n, status)
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> UnaryKernel<f32>
    for NonResamplingFirKernel<f32, TapsType>
{
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || 0.0,
            |accum, sample, tap| unsafe { fadd_fast(accum, fmul_fast(sample, tap)) },
        )
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> UnaryKernel<f32>
    for NonResamplingFirKernel<f32, TapsType>
{
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || 0.0,
            |accum, sample, tap| accum + sample * tap,
        )
    }
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> UnaryKernel<Complex<f32>>
    for NonResamplingFirKernel<Complex<f32>, TapsType>
{
    fn work(
        &self,
        i: &[Complex<f32>],
        o: &mut [Complex<f32>],
    ) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || Complex { re: 0.0, im: 0.0 },
            |accum, sample, tap| Complex {
                re: unsafe { fadd_fast(accum.re, fmul_fast(sample.re, tap)) },
                im: unsafe { fadd_fast(accum.im, fmul_fast(sample.im, tap)) },
            },
        )
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> UnaryKernel<Complex<f32>>
    for NonResamplingFirKernel<Complex<f32>, TapsType>
{
    fn work(
        &self,
        i: &[Complex<f32>],
        o: &mut [Complex<f32>],
    ) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || Complex { re: 0.0, im: 0.0 },
            |accum, sample, tap| Complex {
                re: accum.re + sample.re * tap,
                im: accum.im + sample.im * tap,
            },
        )
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
        assert_eq!(
            kernel.work(&input, &mut output),
            (1, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 14.0);

        let mut output = [];
        assert_eq!(
            kernel.work(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientOutput)
        );

        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (1, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 14.0);

        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 2];
        assert_eq!(
            kernel.work(&input, &mut output),
            (2, 2, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 14.0);
        assert_eq!(output[1], 20.0);
    }

    /// Tests the "terminating condition" where the input is finished and the
    /// kernel has produced everything it can given the input, and has exactly
    /// filled the output buffer.
    #[test]
    fn terminating_condition() {
        let taps = [1.0, 2.0];
        let kernel = NonResamplingFirKernel::new(taps);

        // With 5 input samples and 3 out, we just need more output space
        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (3, 3, ComputationStatus::InsufficientOutput)
        );

        // With 4 input samples and 3 out, we've exactly filled the output
        let input = [1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (3, 3, ComputationStatus::BothSufficient)
        );
    }
}
