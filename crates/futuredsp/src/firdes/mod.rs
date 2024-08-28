//! Filter Design
pub use basic::bandpass;
pub use basic::highpass;
pub use basic::hilbert;
pub use basic::kaiser;
pub use basic::lowpass;
pub use basic::root_raised_cosine;

/// Remez Algorithm
#[cfg(feature = "gpl-code")]
pub mod remez;
#[cfg(feature = "gpl-code")]
mod remez_impl;

mod basic;
