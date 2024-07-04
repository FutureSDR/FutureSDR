//! A signal processing library for SDR and real-time DSP.
#![warn(missing_docs)]
#![no_std]
#![allow(internal_features)]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]

#[macro_use]
extern crate log;

#[macro_use]
extern crate alloc;

pub mod fir;
pub use fir::FirFilter;
pub mod firdes;
pub mod iir;
pub use iir::IirFilter;
pub use iir::IirKernel;
pub mod math;
pub mod polyphase_resampling_fir;
pub use polyphase_resampling_fir::PolyphaseResamplingFir;
pub mod taps;
pub use taps::Taps;
pub mod windows;

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

/// Implements a trait to run computations with stateless kernels.
pub trait FirKernel<InputType, OutputType>: Send {
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
    fn filter(
        &self,
        input: &[InputType],
        output: &mut [OutputType],
    ) -> (usize, usize, ComputationStatus);
}
