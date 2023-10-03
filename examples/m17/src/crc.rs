pub struct Crc;

impl Crc {
    const M17_CRC_POLY: u32 = 0x5935;

    pub fn crc(data: &[u8]) -> u16 {

        let mut crc: u32 = 0xFFFF;

        for i in 0..data.len() {
            crc ^= (data[i] as u32) << 8;
            for _ in 0..8 {
                crc <<= 1;
                if crc & 0x10000 != 0 {
                    crc = (crc ^ Self::M17_CRC_POLY) & 0xFFFF;
                }
            }
        }
        (crc & 0xFFFF) as u16
    }
}
