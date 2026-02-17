//! Decimating FIR Filters
use core::cmp::Ordering;
#[cfg(RUSTC_IS_NIGHTLY)]
use core::intrinsics::fadd_fast;
#[cfg(RUSTC_IS_NIGHTLY)]
use core::intrinsics::fmul_fast;
use num_complex::Complex;
use num_traits::Float;
use num_traits::Zero;

use crate::ComputationStatus;
use crate::Filter;
use crate::Taps;

/// A decimating FIR filter.
///
/// Implementations of this core exist for the following combinations:
/// - `f32` samples, `f32` taps.
/// - `Complex<f32>` samples, `f32` taps.
///
/// Example usage:
/// ```
/// use futuredsp::prelude::*;
/// use futuredsp::DecimatingFirFilter;
///
/// let fir = DecimatingFirFilter::<f32, f32, _>::new(2, [1f32, 2f32, 3f32]);
///
/// let input = [0.0, 1.0, 2.0, 3.0];
/// let mut output = [0.0];
/// fir.filter(&input, &mut output);
/// ```
pub struct DecimatingFirFilter<InputType, OutputType, TA> {
    decimation: usize,
    taps: TA,
    _input_type: core::marker::PhantomData<InputType>,
    _output_type: core::marker::PhantomData<OutputType>,
}

impl<InputType, OutputType, TA> DecimatingFirFilter<InputType, OutputType, TA> {
    /// Create a new non-resampling FIR filter using the given taps.
    pub fn new(decimation: usize, taps: TA) -> Self {
        Self {
            decimation,
            taps,
            _input_type: core::marker::PhantomData,
            _output_type: core::marker::PhantomData,
        }
    }
}

/// Internal helper function to abstract away everything but the core computation.
/// Note that this function gets heavily inlined, so there is no (runtime) performance
/// overhead.
#[inline(always)]
fn fir_kernel_core<
    InputType,
    OutputType,
    TapsType: Taps,
    InitFn: Fn() -> OutputType,
    MacFn: Fn(OutputType, InputType, TapsType::TapType) -> OutputType,
>(
    taps: &TapsType,
    i: &[InputType],
    o: &mut [OutputType],
    decimation: usize,
    init: InitFn,
    mac: MacFn,
) -> (usize, usize, ComputationStatus)
where
    InputType: Copy,
    OutputType: Copy,
    TapsType::TapType: Copy,
{
    let filterable_samples = (i.len() + 1).saturating_sub(taps.num_taps());
    let consumable = filterable_samples / decimation;

    let (n, status) = match consumable.cmp(&o.len()) {
        Ordering::Greater => (o.len(), ComputationStatus::InsufficientOutput),
        Ordering::Equal => (o.len(), ComputationStatus::BothSufficient),
        Ordering::Less => (consumable, ComputationStatus::InsufficientInput),
    };

    unsafe {
        for k in 0..n {
            let mut sum = init();
            for t in 0..taps.num_taps() {
                sum = mac(
                    sum,
                    *i.get_unchecked(decimation - 1 + k * decimation + t),
                    taps.get(taps.num_taps() - 1 - t),
                );
            }
            *o.get_unchecked_mut(k) = sum;
        }
    }

    (n * decimation, n, status)
}

#[cfg(RUSTC_IS_NIGHTLY)]
mod inner {
    use super::*;

    impl<TA: Taps<TapType = f32>> Filter<f32, f32, f32> for DecimatingFirFilter<f32, f32, TA> {
        fn filter(&self, i: &[f32], o: &mut [f32]) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || 0.0,
                |accum, sample, tap| unsafe { fadd_fast(accum, fmul_fast(sample, tap)) },
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }

    impl<TA: Taps<TapType = f64>> Filter<f64, f64, f64> for DecimatingFirFilter<f64, f64, TA> {
        fn filter(&self, i: &[f64], o: &mut [f64]) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || 0.0,
                |accum, sample, tap| unsafe { fadd_fast(accum, fmul_fast(sample, tap)) },
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }

    impl<TA: Taps<TapType = T>, T> Filter<Complex<T>, Complex<T>, T>
        for DecimatingFirFilter<Complex<T>, Complex<T>, TA>
    where
        T: Float + Send + Sync + Copy + Zero,
    {
        fn filter(
            &self,
            i: &[Complex<TA::TapType>],
            o: &mut [Complex<TA::TapType>],
        ) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || Complex {
                    re: T::zero(),
                    im: T::zero(),
                },
                |accum, sample, tap| Complex {
                    re: unsafe { fadd_fast(accum.re, fmul_fast(sample.re, tap)) },
                    im: unsafe { fadd_fast(accum.im, fmul_fast(sample.im, tap)) },
                },
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }
}

#[cfg(not(RUSTC_IS_NIGHTLY))]
mod inner {
    use super::*;

    impl<TA: Taps<TapType = f32>> Filter<f32, f32, f32> for DecimatingFirFilter<f32, f32, TA> {
        fn filter(&self, i: &[f32], o: &mut [f32]) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || 0.0,
                |accum, sample, tap| accum + sample * tap,
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }

    impl<TA: Taps<TapType = f64>> Filter<f64, f64, f64> for DecimatingFirFilter<f64, f64, TA> {
        fn filter(&self, i: &[f64], o: &mut [f64]) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || 0.0,
                |accum, sample, tap| accum + sample * tap,
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }

    impl<TA: Taps<TapType = T>, T> Filter<Complex<T>, Complex<T>, T>
        for DecimatingFirFilter<Complex<T>, Complex<T>, TA>
    where
        T: Float + Send + Sync + Copy + Zero,
    {
        fn filter(
            &self,
            i: &[Complex<T>],
            o: &mut [Complex<T>],
        ) -> (usize, usize, ComputationStatus) {
            fir_kernel_core(
                &self.taps,
                i,
                o,
                self.decimation,
                || Complex {
                    im: T::zero(),
                    re: T::zero(),
                },
                |accum, sample, tap| Complex {
                    re: accum.re + sample.re * tap,
                    im: accum.im + sample.im * tap,
                },
            )
        }
        fn length(&self) -> usize {
            self.taps.num_taps()
        }
    }
}

impl<TA: Taps<TapType = Complex<f32>>> Filter<Complex<f32>, Complex<f32>, Complex<f32>>
    for DecimatingFirFilter<Complex<f32>, Complex<f32>, TA>
{
    fn filter(
        &self,
        i: &[TA::TapType],
        o: &mut [TA::TapType],
    ) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            self.decimation,
            || Complex { re: 0.0, im: 0.0 },
            |accum, sample, tap| accum + sample * tap,
        )
    }
    fn length(&self) -> usize {
        self.taps.num_taps()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimation_one() {
        let taps: [f32; 3] = [1.0, 2.0, 3.0];
        let fir = DecimatingFirFilter::new(1, taps);
        let input = [1.0, 2.0, 3.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (1, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 10.0);

        let mut output = [];
        assert_eq!(
            fir.filter(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientOutput)
        );

        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 2];
        assert_eq!(
            fir.filter(&input, &mut output),
            (2, 2, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 10.0);
        assert_eq!(output[1], 16.0);
    }

    #[test]
    fn decimation_two() {
        let taps: [f32; 3] = [1.0, 2.0, 3.0];
        let fir = DecimatingFirFilter::new(2, taps);
        let input = [0.0, 1.0, 2.0, 3.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (2, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 10.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (2, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 10.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 1];
        assert_eq!(
            fir.filter(&input, &mut output),
            (2, 1, ComputationStatus::BothSufficient)
        );
        assert_eq!(output[0], 10.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 1];
        assert_eq!(
            fir.filter(&input, &mut output),
            (2, 1, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 10.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (4, 2, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 10.0);
        assert_eq!(output[1], 22.0);

        let mut output = [];
        assert_eq!(
            fir.filter(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientOutput)
        );
    }

    #[test]
    fn decimation_three() {
        let taps: [f32; 3] = [1.0, 2.0, 1.0];
        let fir = DecimatingFirFilter::new(3, taps);
        let input = [0.0, 1.0, 2.0, 3.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientInput)
        );

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 12.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 1];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 1, ComputationStatus::BothSufficient)
        );
        assert_eq!(output[0], 12.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 12.0);

        let input = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (6, 2, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 12.0);
        assert_eq!(output[1], 24.0);

        let mut output = [];
        assert_eq!(
            fir.filter(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientOutput)
        );
    }

    /// Tests the "terminating condition" where the input is finished and the
    /// kernel has produced everything it can given the input, and has exactly
    /// filled the output buffer.
    #[test]
    fn terminating_condition() {
        let taps: [f32; 2] = [1.0, 2.0];
        let fir = DecimatingFirFilter::new(1, taps);

        // With 5 input samples and 3 out, we just need more output space
        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 3, ComputationStatus::InsufficientOutput)
        );

        // With 4 input samples and 3 out, we've exactly filled the output
        let input = [1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 3, ComputationStatus::BothSufficient)
        );
    }

    #[test]
    fn terminating_condition_f64() {
        let taps: [f64; 2] = [1.0, 2.0];
        let fir = DecimatingFirFilter::new(1, taps);

        // With 5 input samples and 3 out, we just need more output space
        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 3, ComputationStatus::InsufficientOutput)
        );

        // With 4 input samples and 3 out, we've exactly filled the output
        let input = [1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 3];
        assert_eq!(
            fir.filter(&input, &mut output),
            (3, 3, ComputationStatus::BothSufficient)
        );
    }
}
