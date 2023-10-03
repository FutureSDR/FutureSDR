mod crc;
pub use crc::Crc;

mod encoder;
pub use encoder::Encoder;
pub use encoder::LinkSetupFrame;

mod golay;
pub use golay::Golay;

const PUNCTERING_1: [u8; 61] = [
    1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1,
    1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1,
];
const PUNCTERING_2: [u8; 12] = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0];

const SYNC_LSF: u16 = 0x55F7;
const SYNC_STR: u16 = 0xFF5D;
const SYNC_PKT: u16 = 0x75FF;
const SYNC_BER: u16 = 0xDF55;
const EOT_MRKR: u16 = 0x555D;
