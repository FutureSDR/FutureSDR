use crate::utils::Bandwidth;
use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;

pub const BANDWIDTH: Bandwidth = Bandwidth::BW125;
pub const HAS_CRC: bool = true;
pub const PREAMBLE_LEN: usize = 8;
pub const PAD_SYMBOLS: usize = 0;
pub const CODE_RATE_LORAWAN: CodeRate = CodeRate::CR_4_5;
pub const OVERSAMPLING_TX: usize = 8;
/// fist block of interleaved PHY header and Payload, always uses CR4/8 -> therefore need all 8 symbols of the interleaved block. always contains the full header
pub const INTERLEAVED_HEADER_SYMBOL_COUNT: usize = 8;
pub const SOFT_DECODING: bool = true;
pub const PACKET_FORWARDER_PORT: u16 = 1730;
pub const LDRO_MAX_DURATION_MS: f32 = 16.;

pub fn preamble_len(sf: SpreadingFactor) -> usize {
    if sf == SpreadingFactor::SF5 {
        12
    } else {
        PREAMBLE_LEN
    }
}
pub fn code_rate(sf: SpreadingFactor) -> CodeRate {
    if sf <= SpreadingFactor::SF6 {
        CodeRate::CR_4_8
    } else {
        CodeRate::CR_4_5
    }
}
