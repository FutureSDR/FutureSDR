#![no_std]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]

pub mod fir;

mod tapsaccessor;
pub use tapsaccessor::TapsAccessor;
