pub struct Golay;

impl Golay {
    const ENCODE_MATRIX: [u16; 12] = [
        0x8eb, 0x93e, 0xa97, 0xdc6, 0x367, 0x6cd, 0xd99, 0x3da, 0x7b4, 0xf68, 0x63b, 0xc75,
    ];

    pub fn encode(data: u16) -> u32 {
        let mut checksum: u16 = 0;
        for i in 0..12 {
            if (data & (1 << i)) != 0 {
                checksum ^= Self::ENCODE_MATRIX[i];
            }
        }
        ((data as u32) << 12) | (checksum as u32)
    }
}
