#![no_std]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]

pub mod fir;
pub mod iir;

mod tapsaccessor;
pub use tapsaccessor::TapsAccessor;

mod kernel;
pub use kernel::{ComputationStatus, StatefulUnaryKernel, UnaryKernel};
