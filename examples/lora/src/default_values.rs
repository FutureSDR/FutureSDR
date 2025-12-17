use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;

pub const SYNC_WORD_PUBLIC: usize = 0x34;
pub const SYNC_WORD_PRIVATE: usize = 0x12;

pub const BANDWIDTH: usize = 125_000;
pub const HAS_CRC: bool = true;
pub const IMPLICIT_HEADER: bool = false;
pub const PREAMBLE_LEN: usize = 8;
pub const PAD_SYMBOLS: usize = 0;
pub const CODE_RATE_LORAWAN: CodeRate = CodeRate::CR_4_5;
pub const OVERSAMPLING_TX: usize = 8;
/// fist block of interleaved PHY header and Payload, always uses CR4/8 -> therefore need all 8 symbols of the interleaved block. always contains the full header
pub const INTERLEAVED_HEADER_SYMBOL_COUNT: usize = 8;
pub const SOFT_DECODING: bool = true;
pub const PACKET_FORWARDER_PORT: u16 = 1730;

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
pub fn ldro(sf: SpreadingFactor) -> bool {
    sf >= SpreadingFactor::SF11
}
