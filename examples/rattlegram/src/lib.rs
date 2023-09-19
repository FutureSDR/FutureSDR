#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod bch;
pub use bch::Bch;

mod decoder;
pub use decoder::Decoder;
pub use decoder::DecoderResult;

mod encoder;
pub use encoder::Encoder;

mod mls;
pub use mls::Mls;

mod osd;
pub use osd::OrderedStatisticsDecoder;

mod polar;
pub use polar::PolarEncoder;

mod psk;
pub use psk::Psk;

mod util;
pub use util::get_be_bit;
pub use util::get_le_bit;
pub use util::set_be_bit;
pub use util::set_le_bit;
pub use util::xor_be_bit;
pub use util::OperationMode;

mod xorshift;
pub use xorshift::Xorshift32;
