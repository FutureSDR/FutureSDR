#[cfg(feature = "vulkan")]
mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkan::Vulkan;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

mod keep_1_in_n;
pub use keep_1_in_n::Keep1InN;

use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;

pub fn lin2db_block() -> Block {
    Apply::new(|x: &f32| 10.0 * x.log10())
}

pub fn power_block() -> Block {
    Apply::new(|x: &Complex32| x.norm())
}

pub fn lin2power_db() -> Block {
    Apply::new(|x: &Complex32| 10.0 * x.norm().log10())
}
