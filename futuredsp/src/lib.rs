#![no_std]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]

#[macro_use]
pub extern crate log;

#[macro_use]
extern crate alloc;

pub mod fir;
pub mod firdes;
pub mod iir;
pub mod math;
pub mod windows;

mod tapsaccessor;
pub use tapsaccessor::TapsAccessor;

mod kernel;
pub use kernel::{ComputationStatus, StatefulUnaryKernel, UnaryKernel};
