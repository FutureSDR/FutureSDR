/// Represents the status of a computation.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ComputationStatus {
    /// Indicates that the output buffer could hold more samples, if more
    /// input samples were present.
    InsufficientInput,

    /// Indicates that more output samples can be computed from the given input,
    /// but there is not enough available space in the output buffer.
    InsufficientOutput,

    /// Indicates that as many samples as possible could be computed from the
    /// input buffer, and that the output buffer was exactly filled.
    BothSufficient,
}

impl ComputationStatus {
    /// Returns whether the output was sufficient to hold all producible samples.
    pub fn produced_all_samples(self) -> bool {
        self == Self::BothSufficient || self == Self::InsufficientInput
    }
}

/// Implements a trait to run computations with FIR filters.
pub trait UnaryKernel<SampleType>: Send {
    /// Computes the FIR filter on the given input, outputting into the given output.
    /// Note that filters will not generally have internal memory - therefore, even
    /// if the output is sufficiently large, not all input samples may be consumed.
    /// However, it is also permitted for kernel implementations to contain state
    /// related to the input stream (for example, it may contain an internal buffer
    /// of the last `num_taps` input samples).
    ///
    /// Returns a tuple containing, in order:
    /// - The number of samples consumed from the input,
    /// - The number of samples produced in the output, and
    /// - A `ComputationStatus` which indicates whether the buffers were undersized.
    ///
    /// Elements of `output` beyond what is produced are left in an unspecified state.
    fn work(
        &self,
        input: &[SampleType],
        output: &mut [SampleType],
    ) -> (usize, usize, ComputationStatus);
}

/// Implements a trait to run computations with FIR filters.
pub trait StatefulUnaryKernel<SampleType>: Send {
    /// Computes the FIR filter on the given input, outputting into the given output.
    /// Note that filters will not generally have internal memory - therefore, even
    /// if the output is sufficiently large, not all input samples may be consumed.
    /// However, it is also permitted for kernel implementations to contain state
    /// related to the input stream (for example, it may contain an internal buffer
    /// of the last `num_taps` input samples).
    ///
    /// Returns a tuple containing, in order:
    /// - The number of samples consumed from the input,
    /// - The number of samples produced in the output, and
    /// - A `ComputationStatus` which indicates whether the buffers were undersized.
    ///
    /// Elements of `output` beyond what is produced are left in an unspecified state.
    fn work(
        &mut self,
        input: &[SampleType],
        output: &mut [SampleType],
    ) -> (usize, usize, ComputationStatus);
}

impl<SampleType, T: UnaryKernel<SampleType>> StatefulUnaryKernel<SampleType> for T {
    fn work(
        &mut self,
        input: &[SampleType],
        output: &mut [SampleType],
    ) -> (usize, usize, ComputationStatus) {
        UnaryKernel::<SampleType>::work(self, input, output)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct NopKernel;

    impl UnaryKernel<f32> for NopKernel {
        fn work(&self, _input: &[f32], output: &mut [f32]) -> (usize, usize, ComputationStatus) {
            for i in 0..output.len() {
                output[i] = i as f32;
            }
            (0, output.len(), ComputationStatus::BothSufficient)
        }
    }

    fn exec_kernel<T: StatefulUnaryKernel<f32>>(mut kernel: T, output: &mut [f32]) {
        kernel.work(&[], output);
    }

    #[test]
    fn call_stateful_unary_on_unary_test() {
        let kernel = NopKernel;
        let mut output = [0.0; 4];
        exec_kernel(kernel, &mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn can_naturally_resolve_mut_kernel() {
        #[allow(unused_mut)]
        let mut kernel = NopKernel;
        let mut output = [0.0; 4];
        kernel.work(&[], &mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn can_naturally_resolve_const_kernel() {
        let kernel = NopKernel;
        let mut output = [0.0; 4];
        kernel.work(&[], &mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
    }
}
