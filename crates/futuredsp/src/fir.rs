use core::cmp::Ordering;
#[cfg(not(RUSTC_IS_STABLE))]
use core::intrinsics::{fadd_fast, fmul_fast};

use crate::{ComputationStatus, TapsAccessor, UnaryKernel};
use num_complex::Complex;
use num_traits::{Float, Zero};

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
/// let fir = NonResamplingFirKernel::<f32, f32, _, _>::new([1.0, 2.0, 3.0]);
///
/// let input = [1.0, 2.0, 3.0];
/// let mut output = [0.0];
/// fir.work(&input, &mut output);
/// ```
pub struct NonResamplingFirKernel<InputType, OutputType, TA, TT>
where
    TA: TapsAccessor<TapType = TT>,
{
    taps: TA,
    _input_type: core::marker::PhantomData<InputType>,
    _output_type: core::marker::PhantomData<OutputType>,
}

impl<InputType, OutputType, TA, TT> NonResamplingFirKernel<InputType, OutputType, TA, TT>
where
    TA: TapsAccessor<TapType = TT>,
{
    /// Create a new non-resampling FIR filter using the given taps.
    pub fn new(taps: TA) -> Self {
        Self {
            taps,
            _input_type: core::marker::PhantomData,
            _output_type: core::marker::PhantomData,
        }
    }
}

/// Internal helper function to abstract away everything but the core computation.
/// Note that this function gets heavily inlined, so there is no (runtime) performance
/// overhead.
fn fir_kernel_core<
    InputType,
    OutputType,
    TapsType: TapsAccessor,
    InitFn: Fn() -> OutputType,
    MacFn: Fn(OutputType, InputType, TapsType::TapType) -> OutputType,
>(
    taps: &TapsType,
    i: &[InputType],
    o: &mut [OutputType],
    init: InitFn,
    mac: MacFn,
) -> (usize, usize, ComputationStatus)
where
    InputType: Copy,
    OutputType: Copy,
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
                sum = mac(
                    sum,
                    *i.get_unchecked(k + t),
                    taps.get(taps.num_taps() - 1 - t),
                );
            }
            *o.get_unchecked_mut(k) = sum;
        }
    }

    (n, n, status)
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TA: TapsAccessor<TapType = f32>> UnaryKernel<f32, f32>
    for NonResamplingFirKernel<f32, f32, TA, f32>
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
impl<TA: TapsAccessor<TapType = f32>> UnaryKernel<f32, f32>
    for NonResamplingFirKernel<f32, f32, TA, f32>
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
impl<TA: TapsAccessor<TapType = f64>> UnaryKernel<f64, f64>
    for NonResamplingFirKernel<f64, f64, TA, f64>
{
    fn work(&self, i: &[f64], o: &mut [f64]) -> (usize, usize, ComputationStatus) {
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
impl<TA: TapsAccessor<TapType = f64>> UnaryKernel<f64, f64>
    for NonResamplingFirKernel<f64, f64, TA, f64>
{
    fn work(&self, i: &[f64], o: &mut [f64]) -> (usize, usize, ComputationStatus) {
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
impl<TA: TapsAccessor<TapType = T>, T> UnaryKernel<Complex<T>, Complex<T>>
    for NonResamplingFirKernel<Complex<T>, Complex<T>, TA, T>
where
    T: Float + Send + Sync + Copy + Zero,
{
    fn work(&self, i: &[Complex<T>], o: &mut [Complex<T>]) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || Complex {
                im: T::zero(),
                re: T::zero(),
            },
            |accum, sample, tap| Complex {
                re: unsafe { fadd_fast(accum.re, fmul_fast(sample.re, tap)) },
                im: unsafe { fadd_fast(accum.im, fmul_fast(sample.im, tap)) },
            },
        )
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TA: TapsAccessor<TapType = T>, T> UnaryKernel<Complex<T>, Complex<T>>
    for NonResamplingFirKernel<Complex<T>, Complex<T>, TA, T>
where
    T: Float + Send + Sync + Copy + Zero,
{
    fn work(&self, i: &[Complex<T>], o: &mut [Complex<T>]) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
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
}

impl<TA: TapsAccessor<TapType = Complex<T>>, T> UnaryKernel<Complex<T>, Complex<T>>
    for NonResamplingFirKernel<Complex<T>, Complex<T>, TA, Complex<T>>
where
    T: Float + Send + Sync + Copy + Zero,
{
    fn work(&self, i: &[Complex<T>], o: &mut [Complex<T>]) -> (usize, usize, ComputationStatus) {
        fir_kernel_core(
            &self.taps,
            i,
            o,
            || Complex {
                im: T::zero(),
                re: T::zero(),
            },
            |accum, sample, tap| accum + sample * tap,
        )
    }
}

/// A rational resampling polyphase FIR filter. For every input value, this filter
/// produces `interp/decim` output samples. The length of `taps` must be divisible by `interp`.
/// For the best performance, `interp` and `decim` should be relatively prime.
///
/// If `decim=1`, then the filter is a pure interpolator. If `interp=1`, then the filter
/// is a pure decimator.
///
/// The specified FIR filter `H(z)` is split into `interp` polyphase components
/// `E_0(z), E_1(z), ..., E_(interp-1)(z)`, such that
/// `H(z) = E_0(z^interp) + z^(-1)E_1(z^interp) + ... + z^(-(interp-1))E_(interp-1)(z^interp)`
/// The taps for each polyphase component are given by `e_l(n) = h(l*n+l)` for `0 <= l <= interp-1`.
///
/// Implementations of this core exist for the following combinations:
/// - `f32` samples, `f32` taps.
/// - `Complex<f32>` samples, `f32` taps.
///
/// Example usage:
/// ```
/// use futuredsp::UnaryKernel;
/// use futuredsp::fir::PolyphaseResamplingFirKernel;
///
/// let decim = 2;
/// let interp = 3;
/// let taps = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
/// let fir = PolyphaseResamplingFirKernel::<f32, f32, _, _>::new(interp, decim, taps);
///
/// let input = [1.0, 2.0, 3.0];
/// let mut output = [0.0; 3];
/// fir.work(&input, &mut output);
/// ```
pub struct PolyphaseResamplingFirKernel<InputType, OutputType, TA, TT>
where
    TA: TapsAccessor<TapType = TT>,
{
    interp: usize,
    decim: usize,
    taps: TA,
    _input_type: core::marker::PhantomData<InputType>,
    _output_type: core::marker::PhantomData<OutputType>,
}

impl<InputType, OutputType, TA, TT> PolyphaseResamplingFirKernel<InputType, OutputType, TA, TT>
where
    TA: TapsAccessor<TapType = TT>,
{
    /// Create a new resampling FIR filter using the given filter bank taps.
    pub fn new(interp: usize, decim: usize, taps: TA) -> Self {
        // Ensure number of taps is divisible by interp
        assert!(taps.num_taps() % interp == 0);
        Self {
            interp,
            decim,
            taps,
            _input_type: core::marker::PhantomData,
            _output_type: core::marker::PhantomData,
        }
    }
}

/// Internal helper function to abstract away everything but the core computation.
/// Note that this function gets heavily inlined, so there is no (runtime) performance
/// overhead.
fn resampling_fir_kernel_core<
    InputType,
    OutputType,
    TapsType: TapsAccessor,
    InitFn: Fn() -> OutputType,
    MacFn: Fn(OutputType, InputType, TapsType::TapType) -> OutputType,
>(
    interp: usize,
    decim: usize,
    taps: &TapsType,
    i: &[InputType],
    o: &mut [OutputType],
    init: InitFn,
    mac: MacFn,
) -> (usize, usize, ComputationStatus)
where
    InputType: Copy,
    OutputType: Copy,
    TapsType::TapType: Copy,
{
    // Assume same number of taps in all filters
    let num_taps = taps.num_taps() / interp;
    let num_producable_samples =
        ((i.len() + 1).saturating_sub(num_taps) * interp).saturating_sub(1) / decim;
    // Ensure it is divisible by interpolation factor to avoid keeping track of state
    let num_producable_samples = (num_producable_samples / interp) * interp;
    let (num_producable_samples, status) = match num_producable_samples.cmp(&o.len()) {
        Ordering::Greater => (
            (o.len() / interp) * interp,
            ComputationStatus::InsufficientOutput,
        ),
        Ordering::Equal => (num_producable_samples, ComputationStatus::BothSufficient),
        Ordering::Less => (num_producable_samples, ComputationStatus::InsufficientInput),
    };
    // Compute number of input samples to consume
    //let n = num_producable_samples.saturating_sub(1) * decim / interp + 1;
    let n = (num_producable_samples / interp) * decim;

    unsafe {
        for k in 0..num_producable_samples {
            let bank_idx = (k * decim) % interp;
            let input_idx = k * decim / interp;
            let mut sum = init();
            for t in 0..num_taps {
                let tap_idx = interp * (num_taps - t - 1) + bank_idx;
                sum = mac(sum, *i.get_unchecked(input_idx + t), taps.get(tap_idx));
            }
            *o.get_unchecked_mut(k) = sum;
        }
    }
    // Assert state is 0 so that we do not need to keep track of the state
    debug_assert!(((num_producable_samples * decim) % interp) == 0);

    (n, num_producable_samples, status)
}

impl<TA: TapsAccessor<TapType = f32>> UnaryKernel<f32, f32>
    for PolyphaseResamplingFirKernel<f32, f32, TA, f32>
{
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize, ComputationStatus) {
        resampling_fir_kernel_core(
            self.interp,
            self.decim,
            &self.taps,
            i,
            o,
            || 0.0,
            |accum, sample, tap| accum + sample * tap,
        )
    }
}

impl<TA: TapsAccessor<TapType = f32>> UnaryKernel<Complex<f32>, Complex<f32>>
    for PolyphaseResamplingFirKernel<Complex<f32>, Complex<f32>, TA, f32>
{
    fn work(
        &self,
        i: &[Complex<f32>],
        o: &mut [Complex<f32>],
    ) -> (usize, usize, ComputationStatus) {
        resampling_fir_kernel_core(
            self.interp,
            self.decim,
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

impl<TA: TapsAccessor<TapType = f64>> UnaryKernel<f64, f64>
    for PolyphaseResamplingFirKernel<f64, f64, TA, f64>
{
    fn work(&self, i: &[f64], o: &mut [f64]) -> (usize, usize, ComputationStatus) {
        resampling_fir_kernel_core(
            self.interp,
            self.decim,
            &self.taps,
            i,
            o,
            || 0.0,
            |accum, sample, tap| accum + sample * tap,
        )
    }
}

impl<TA: TapsAccessor<TapType = f64>> UnaryKernel<Complex<f64>, Complex<f64>>
    for PolyphaseResamplingFirKernel<Complex<f64>, Complex<f64>, TA, f64>
{
    fn work(
        &self,
        i: &[Complex<f64>],
        o: &mut [Complex<f64>],
    ) -> (usize, usize, ComputationStatus) {
        resampling_fir_kernel_core(
            self.interp,
            self.decim,
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
        let taps: [f32; 3] = [1.0, 2.0, 3.0];
        let kernel = NonResamplingFirKernel::new(taps);
        let input = [1.0, 2.0, 3.0];
        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (1, 1, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 10.0);

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
        assert_eq!(output[0], 10.0);

        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 2];
        assert_eq!(
            kernel.work(&input, &mut output),
            (2, 2, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 10.0);
        assert_eq!(output[1], 16.0);
    }

    /// Tests the "terminating condition" where the input is finished and the
    /// kernel has produced everything it can given the input, and has exactly
    /// filled the output buffer.
    #[test]
    fn terminating_condition() {
        let taps: [f32; 2] = [1.0, 2.0];
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

    #[test]
    fn terminating_condition_f64() {
        let taps: [f64; 2] = [1.0, 2.0];
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

    #[test]
    fn direct_resampling_fir_kernel() {
        let interp = 3;
        let decim = 2;
        let taps: [f32; 6] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let kernel = PolyphaseResamplingFirKernel::new(interp, decim, taps);
        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut output = [0.0; 8];
        assert_eq!(
            kernel.work(&input, &mut output),
            (2, 3, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 6.0);
        assert_eq!(output[1], 12.0);
        assert_eq!(output[2], 16.0);

        let mut output = [];
        assert_eq!(
            kernel.work(&input, &mut output),
            (0, 0, ComputationStatus::InsufficientOutput)
        );

        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (2, 3, ComputationStatus::BothSufficient)
        );
        assert_eq!(output[0], 6.0);
        assert_eq!(output[1], 12.0);
        assert_eq!(output[2], 16.0);

        // With 3 input samples and 3 out, we've exactly filled the output
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut output = [0.0; 3];
        assert_eq!(
            kernel.work(&input, &mut output),
            (2, 3, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 6.0);
        assert_eq!(output[1], 12.0);
        assert_eq!(output[2], 16.0);
        let input = &input[2..input.len()];
        assert_eq!(
            kernel.work(input, &mut output),
            (2, 3, ComputationStatus::InsufficientOutput)
        );
        assert_eq!(output[0], 16.0);
        assert_eq!(output[1], 30.0);
        assert_eq!(output[2], 30.0);
        let input = &input[2..input.len()];
        assert_eq!(
            kernel.work(input, &mut output),
            (2, 3, ComputationStatus::BothSufficient)
        );
        assert_eq!(output[0], 26.0);
        assert_eq!(output[1], 48.0);
        assert_eq!(output[2], 44.0);

        let interp = 2;
        let decim = 1;
        let taps: [f32; 2] = [1.0, 2.0];
        let kernel = PolyphaseResamplingFirKernel::new(interp, decim, taps);
        let input = [1.0, 2.0, 3.0, 4.0];
        let mut output = [0.0; 10];
        assert_eq!(
            kernel.work(&input, &mut output),
            (3, 6, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 1.0);
        assert_eq!(output[1], 2.0);
        assert_eq!(output[2], 2.0);
        assert_eq!(output[3], 4.0);
        assert_eq!(output[4], 3.0);
        assert_eq!(output[5], 6.0);

        let interp = 1;
        let decim = 3;
        let taps: [f32; 2] = [1.0, 2.0];
        let kernel = PolyphaseResamplingFirKernel::new(interp, decim, taps);
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut output = [0.0; 8];
        assert_eq!(
            kernel.work(&input, &mut output),
            (6, 2, ComputationStatus::InsufficientInput)
        );
        assert_eq!(output[0], 4.0);
        assert_eq!(output[1], 13.0);
    }
}
