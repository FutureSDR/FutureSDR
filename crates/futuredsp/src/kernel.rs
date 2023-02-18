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

/// Implements a trait to run computations with stateless kernels.
pub trait UnaryKernel<InputType, OutputType>: Send {
    /// Computes the kernel on the given input, outputting into the given
    /// output. For a `UnaryKernel`, kernels will not have internal memory - in
    /// particular, this means that a single instantiated kernel does not need
    /// to be reserved for a single stream of data.
    ///
    /// Returns a tuple containing, in order:
    /// - The number of samples consumed from the input,
    /// - The number of samples produced in the output, and
    /// - A `ComputationStatus` which indicates whether the buffers were undersized.
    ///
    /// Elements of `output` beyond what is produced are left in an unspecified state.
    fn work(
        &self,
        input: &[InputType],
        output: &mut [OutputType],
    ) -> (usize, usize, ComputationStatus);
}

/// Implements a trait to run computations with stateful kernels.
pub trait StatefulUnaryKernel<InputType, OutputType>: Send {
    /// Computes the kernel on the given input, outputting into the given
    /// output. `StatefulUnaryKernel`s have internal state. This results in
    /// several properties:
    /// * Even if the output is sufficiently large, not all input samples may be
    ///   consumed.
    /// * A given instantiated kernel must be called sequentially on a single
    ///   stream of data. Switching between data streams with a single kernel
    ///   will result in undefined behaviour.
    ///
    /// Note that all `StatefulUnaryKernel`s implement `UnaryKernel` by
    /// definition.
    ///
    /// Returns a tuple containing, in order:
    /// - The number of samples consumed from the input,
    /// - The number of samples produced in the output, and
    /// - A `ComputationStatus` which indicates whether the buffers were undersized.
    ///
    /// Elements of `output` beyond what is produced are left in an unspecified state.
    fn work(
        &mut self,
        input: &[InputType],
        output: &mut [OutputType],
    ) -> (usize, usize, ComputationStatus);
}

impl<InputType, OutputType, T: UnaryKernel<InputType, OutputType>>
    StatefulUnaryKernel<InputType, OutputType> for T
{
    fn work(
        &mut self,
        input: &[InputType],
        output: &mut [OutputType],
    ) -> (usize, usize, ComputationStatus) {
        UnaryKernel::<InputType, OutputType>::work(self, input, output)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct NopKernel;

    impl UnaryKernel<f32, f32> for NopKernel {
        fn work(&self, _input: &[f32], output: &mut [f32]) -> (usize, usize, ComputationStatus) {
            for (i, out) in output.iter_mut().enumerate() {
                *out = i as f32;
            }
            (0, output.len(), ComputationStatus::BothSufficient)
        }
    }

    fn exec_kernel<T: StatefulUnaryKernel<f32, f32>>(mut kernel: T, output: &mut [f32]) {
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
