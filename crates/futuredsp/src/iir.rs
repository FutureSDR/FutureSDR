use core::ops::{AddAssign, Mul};

use crate::{ComputationStatus, StatefulUnaryKernel, TapsAccessor};

extern crate alloc;
use alloc::vec::Vec;
use num_traits::Zero;

/// An IIR filter.
///
/// Calling `work()` on this struct always produces exactly as many samples as
/// it consumes. Note that this kernel is stateful, and thus implements the
/// [StatefulUnaryKernel] trait.
///
/// Implementations of this core currently exist only for `f32` samples with
/// `f32` taps.
///
/// Example usage:
/// ```
/// use futuredsp::StatefulUnaryKernel;
/// use futuredsp::iir::IirKernel;
///
/// let mut iir = IirKernel::<f32, f32, _>::new([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]);
///
/// let input = [1.0, 2.0, 3.0, 4.0, 5.0];
/// let mut output = [0.0];
/// iir.work(&input, &mut output);
/// assert_eq!(output[0], 42.0);
/// ```
pub struct IirKernel<InputType, OutputType, TapsType: TapsAccessor> {
    a_taps: TapsType,
    b_taps: TapsType,
    memory: Vec<InputType>,
    _input_type: core::marker::PhantomData<InputType>,
    _output_type: core::marker::PhantomData<OutputType>,
}

impl<InputType, OutputType, TapType, TapsType: TapsAccessor<TapType = TapType>>
    IirKernel<InputType, OutputType, TapsType>
{
    pub fn new(a_taps: TapsType, b_taps: TapsType) -> Self {
        Self {
            a_taps,
            b_taps,
            memory: Vec::new(),
            _input_type: core::marker::PhantomData,
            _output_type: core::marker::PhantomData,
        }
    }
}

impl<TapsType: TapsAccessor<TapType = f32>> StatefulUnaryKernel<f32, f32>
    for IirKernel<f32, f32, TapsType>
{
    fn work(&mut self, input: &[f32], output: &mut [f32]) -> (usize, usize, ComputationStatus) {
        taps_accessor_work(&mut self.memory, &self.a_taps, &self.b_taps, input, output)
    }
}

impl<TapsType: TapsAccessor<TapType = f64>> StatefulUnaryKernel<f64, f64>
    for IirKernel<f64, f64, TapsType>
{
    fn work(&mut self, input: &[f64], output: &mut [f64]) -> (usize, usize, ComputationStatus) {
        taps_accessor_work(&mut self.memory, &self.a_taps, &self.b_taps, input, output)
    }
}

#[inline(always)]
fn taps_accessor_work<TT, T>(
    memory: &mut Vec<T>,
    a_taps: &TT,
    b_taps: &TT,
    i: &[T],
    o: &mut [T],
) -> (usize, usize, ComputationStatus)
where
    TT: TapsAccessor<TapType = T>,
    T: Copy + AddAssign + Zero + Mul<Output = T>,
{
    if i.is_empty() {
        return (
            0,
            0,
            if o.is_empty() {
                ComputationStatus::BothSufficient
            } else {
                ComputationStatus::InsufficientInput
            },
        );
    }

    // Load the memory with samples
    let mut num_filled = 0;
    while memory.len() < a_taps.num_taps() {
        if i.len() <= memory.len() {
            return (
                0,
                0,
                if o.is_empty() {
                    ComputationStatus::BothSufficient
                } else {
                    ComputationStatus::InsufficientInput
                },
            );
        }
        memory.push(i[memory.len()]);
        num_filled += 1;
    }
    if num_filled == i.len() {
        return (
            0,
            0,
            if o.is_empty() {
                ComputationStatus::BothSufficient
            } else {
                ComputationStatus::InsufficientInput
            },
        );
    }

    assert_eq!(a_taps.num_taps(), memory.len());
    assert!(b_taps.num_taps() > 0);

    let mut n_consumed = 0;
    let mut n_produced = 0;
    while n_consumed + b_taps.num_taps() - 1 < i.len() && n_produced < o.len() {
        let o: &mut T = &mut o[n_produced];

        *o = T::zero();

        // Calculate the intermediate value
        for b_tap in 0..b_taps.num_taps() {
            // Safety: We're iterating only up to the # of taps in B
            *o += unsafe { b_taps.get(b_tap) } * i[n_consumed + b_taps.num_taps() - b_tap - 1];
        }

        // Apply the feedback a taps
        #[allow(clippy::needless_range_loop)]
        for a_tap in 0..a_taps.num_taps() {
            // Safety: The iterand is limited to a_taps' length
            *o += unsafe { a_taps.get(a_tap) } * memory[a_tap];
        }

        // Update the memory
        for idx in 1..memory.len() {
            memory[idx] = memory[idx - 1];
        }
        if !memory.is_empty() {
            memory[0] = *o;
        }

        n_produced += 1;
        n_consumed += 1;
    }

    (
        n_consumed,
        n_produced,
        if n_consumed == i.len() && n_produced == o.len() {
            ComputationStatus::BothSufficient
        } else if n_consumed < i.len() {
            ComputationStatus::InsufficientOutput
        } else {
            assert!(n_produced < o.len());
            ComputationStatus::InsufficientInput
        },
    )
}

#[cfg(test)]
mod test {
    use super::*;

    use alloc::vec;

    struct Feeder {
        filter: IirKernel<f32, f32, Vec<f32>>,
        input: Vec<f32>,
    }

    impl Feeder {
        fn feed(&mut self, input: f32) -> Option<f32> {
            self.input.push(input);

            let mut out = [0.0];
            let (n_consumed, n_produced, _status) = self.filter.work(&self.input[..], &mut out);
            assert_eq!(n_consumed, n_produced); // If we consume samples, we produce samples
            if n_consumed > 0 {
                self.input.drain(0..n_consumed);
            }
            if n_produced > 0 {
                Some(out[0])
            } else {
                None
            }
        }
    }

    fn make_filter(a_taps: Vec<f32>, b_taps: Vec<f32>) -> Feeder {
        Feeder {
            filter: IirKernel {
                a_taps,
                b_taps,
                memory: vec![],
                _input_type: core::marker::PhantomData,
                _output_type: core::marker::PhantomData,
            },
            input: vec![],
        }
    }

    #[test]
    fn test_iir_b_taps_algorithm() {
        let mut iir = make_filter(vec![], vec![1.0, 2.0, 3.0]);

        assert_eq!(iir.feed(10.0), None);
        assert_eq!(iir.feed(20.0), None);
        assert_eq!(iir.feed(30.0), Some(30.0 + 40.0 + 30.0));
        assert_eq!(iir.feed(40.0), Some(40.0 + 60.0 + 60.0));
    }

    #[test]
    fn test_iir_single_a_tap_algorithm() {
        let mut iir = make_filter(vec![0.5], vec![1.0]);

        assert_eq!(iir.feed(10.0), None);
        assert_eq!(iir.feed(10.0), Some(15.0));
        assert_eq!(iir.feed(10.0), Some(17.5));
        assert_eq!(iir.feed(10.0), Some(18.75));
    }
}
