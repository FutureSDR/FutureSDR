#[cfg(feature = "vulkan")]
mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkan::Vulkan;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;

pub fn lin2db_block() -> Block {
    Apply::new(|x: &f32| 10.0 * x.log10()).into()
}

pub fn power_block() -> Block {
    Apply::new(|x: &Complex32| x.norm_sqr()).into()
}

pub fn lin2power_db() -> Block {
    Apply::new(|x: &Complex32| 10.0 * x.norm_sqr().log10()).into()
}
