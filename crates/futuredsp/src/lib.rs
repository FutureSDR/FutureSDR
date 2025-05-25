//! A signal processing library for SDR and real-time DSP.
#![warn(missing_docs)]
#![no_std]
#![allow(internal_features)]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate tracing;

pub use num_complex;
pub use num_traits;

pub use decimating_fir::DecimatingFirFilter;
pub use fir::FirFilter;
pub use iir::IirFilter;
#[cfg(feature = "gpl-code")]
pub use mmse::MmseResampler;
pub use polyphase_resampling_fir::PolyphaseResamplingFir;
pub use rotator::Rotator;
pub use taps::Taps;

mod decimating_fir;
mod fir;
pub mod firdes;
pub mod iir;
pub mod math;
#[cfg(feature = "gpl-code")]
mod mmse;
mod polyphase_resampling_fir;
pub mod rotator;
pub mod taps;
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

/// Trait for a state-less filter
pub trait Filter<InputType, OutputType, TapType> {
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
    /// Returns the filter length, i.e., the number of input samples required to compute an
    /// output.
    fn length(&self) -> usize;
}

/// Trait for a stateful filter
pub trait StatefulFilter<InputType, OutputType, TapType> {
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
        &mut self,
        input: &[InputType],
        output: &mut [OutputType],
    ) -> (usize, usize, ComputationStatus);
    /// Returns the filter length, i.e., the number of input samples required to compute an
    /// output.
    fn length(&self) -> usize;
}

/// Prelude with common traits
pub mod prelude {
    pub use num_traits;

    pub use super::ComputationStatus;
    pub use super::Filter;
    pub use super::StatefulFilter;
    pub use super::Taps;
}
