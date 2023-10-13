#![allow(clippy::neg_cmp_op_on_partial_ord)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::new_ret_no_self)]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod bch;
pub use bch::Bch;

mod decoder;
pub use decoder::Decoder;
pub use decoder::DecoderBlock;
pub use decoder::DecoderResult;

mod encoder;
pub use encoder::Encoder;

mod mls;
pub use mls::Mls;

mod osd;
pub use osd::OrderedStatisticsDecoder;

mod polar;
pub use polar::PolarDecoder;
pub use polar::PolarEncoder;

mod psk;
pub use psk::Psk;

#[cfg(target_arch = "wasm32")]
pub mod wasm_tx;

mod util;
pub use util::get_be_bit;
pub use util::get_le_bit;
pub use util::set_be_bit;
pub use util::set_le_bit;
pub use util::xor_be_bit;
pub use util::OperationMode;

mod xorshift;
pub use xorshift::Xorshift32;
