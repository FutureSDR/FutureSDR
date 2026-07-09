use crate::ble_protocol::BleAdvPduType;
use crate::ble_protocol::BleParseError;

pub const BLE_ACCESS_ADDRESS: u32 = 0x8E89_BED6;
pub const BLE_ADV_CRC_INIT: u32 = 0x55_55_55;
pub const BLE_MAX_ADV_PAYLOAD_LEN: usize = 37;

#[derive(Clone, Debug)]
pub struct WhiteningState {
    lfsr: u8,
}

impl WhiteningState {
    pub fn new(channel_index: u8) -> Result<Self, BleParseError> {
        if channel_index >= 40 {
            return Err(BleParseError::InvalidChannel(channel_index));
        }

        Ok(Self {
            lfsr: whitening_seed(channel_index),
        })
    }

    pub fn apply_byte(&mut self, byte: u8) -> u8 {
        let mut out = 0u8;
        for bit_index in 0..8 {
            let whitening_bit = self.lfsr & 1;
            let input_bit = (byte >> bit_index) & 1;
            out |= (input_bit ^ whitening_bit) << bit_index;

            let feedback = ((self.lfsr >> 4) ^ self.lfsr) & 1;
            self.lfsr = (self.lfsr >> 1) | (feedback << 6);
        }
        out
    }
}

fn whitening_seed(channel_index: u8) -> u8 {
    let bit = |n| (channel_index >> n) & 1u8;

    bit(0)
        | (bit(1) << 1)
        | (bit(2) << 2)
        | ((bit(0) ^ bit(3)) << 3)
        | ((bit(1) ^ bit(4)) << 4)
        | ((bit(2) ^ bit(5)) << 5)
        | ((1u8 ^ bit(0) ^ bit(3)) << 6)
}

pub fn apply_whitening(channel_index: u8, bytes: &[u8]) -> Result<Vec<u8>, BleParseError> {
    let mut whitening = WhiteningState::new(channel_index)?;
    Ok(bytes
        .iter()
        .map(|&byte| whitening.apply_byte(byte))
        .collect())
}

pub fn crc24_ble(data: &[u8], init: u32) -> u32 {
    let mut crc = reverse_bits(init & 0x00ff_ffff, 24);

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x00da_6000;
            } else {
                crc >>= 1;
            }
        }
    }

    crc & 0x00ff_ffff
}

pub fn frequency_hz_from_channel_index(channel_index: u8) -> Result<f64, BleParseError> {
    let mhz = match channel_index {
        37 => 2402,
        38 => 2426,
        39 => 2480,
        0..=10 => 2404 + channel_index as u32 * 2,
        11..=36 => 2428 + (channel_index as u32 - 11) * 2,
        _ => return Err(BleParseError::InvalidChannel(channel_index)),
    };

    Ok(mhz as f64 * 1.0e6)
}

pub fn generate_ble_packet_bits(channel_index: u8) -> Vec<u8> {
    let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let adv_data = [0x02, 0x01, 0x06];
    let whitened_pdu_crc = build_advertising_pdu_crc(
        channel_index,
        BleAdvPduType::AdvNonconnInd,
        &adv_a,
        &adv_data,
    );

    let mut bits = Vec::new();
    push_byte_bits_lsb_first(&mut bits, 0xaa);
    push_u32_bits_lsb_first(&mut bits, BLE_ACCESS_ADDRESS);
    for byte in whitened_pdu_crc {
        push_byte_bits_lsb_first(&mut bits, byte);
    }
    bits
}

pub fn build_advertising_pdu_crc(
    channel_index: u8,
    pdu_type: BleAdvPduType,
    adv_a: &[u8; 6],
    adv_data: &[u8],
) -> Vec<u8> {
    let pdu_type = match pdu_type {
        BleAdvPduType::AdvInd => 0,
        BleAdvPduType::AdvDirectInd => 1,
        BleAdvPduType::AdvNonconnInd => 2,
        BleAdvPduType::ScanReq => 3,
        BleAdvPduType::ScanRsp => 4,
        BleAdvPduType::ConnectInd => 5,
        BleAdvPduType::AdvScanInd => 6,
        BleAdvPduType::AdvExtInd => 7,
        BleAdvPduType::Reserved(_) => 2,
    };

    let payload_len = adv_a.len() + adv_data.len();
    assert!(payload_len <= BLE_MAX_ADV_PAYLOAD_LEN);

    let mut pdu = Vec::with_capacity(2 + payload_len + 3);
    pdu.push(pdu_type);
    pdu.push(payload_len as u8);
    pdu.extend_from_slice(adv_a);
    pdu.extend_from_slice(adv_data);

    let crc = crc24_ble(&pdu, BLE_ADV_CRC_INIT);
    pdu.push((crc & 0xff) as u8);
    pdu.push(((crc >> 8) & 0xff) as u8);
    pdu.push(((crc >> 16) & 0xff) as u8);

    apply_whitening(channel_index, &pdu).expect("valid test BLE channel")
}

fn reverse_bits(mut value: u32, bits: usize) -> u32 {
    let mut out = 0u32;
    for _ in 0..bits {
        out = (out << 1) | (value & 1);
        value >>= 1;
    }
    out
}

fn push_byte_bits_lsb_first(bits: &mut Vec<u8>, byte: u8) {
    for bit_index in 0..8 {
        bits.push((byte >> bit_index) & 1);
    }
}

fn push_u32_bits_lsb_first(bits: &mut Vec<u8>, value: u32) {
    for bit_index in 0..32 {
        bits.push(((value >> bit_index) & 1) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc24_matches_ble_check_value() {
        assert_eq!(crc24_ble(b"123456789", BLE_ADV_CRC_INIT), 0xC25A56);
    }

    #[test]
    fn whitening_round_trips() {
        let input = [
            0x02, 0x09, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x02, 0x01, 0x06,
        ];
        let whitened = apply_whitening(37, &input).unwrap();
        let dewhitened = apply_whitening(37, &whitened).unwrap();
        assert_eq!(dewhitened, input);
    }

    #[test]
    fn whitening_matches_ble_advertising_channels() {
        assert_eq!(
            apply_whitening(37, &[0x00, 0x00, 0x00, 0x00]).unwrap(),
            [0x8D, 0xD2, 0x57, 0xA1]
        );
        assert_eq!(
            apply_whitening(38, &[0x00, 0x00, 0x00, 0x00]).unwrap(),
            [0xD6, 0xC5, 0x44, 0x20]
        );
        assert_eq!(
            apply_whitening(39, &[0x00, 0x00, 0x00, 0x00]).unwrap(),
            [0x1F, 0x37, 0x4A, 0x5F]
        );
    }

    #[test]
    fn maps_ble_channels_to_frequencies() {
        assert_eq!(frequency_hz_from_channel_index(37).unwrap(), 2.402e9);
        assert_eq!(frequency_hz_from_channel_index(38).unwrap(), 2.426e9);
        assert_eq!(frequency_hz_from_channel_index(39).unwrap(), 2.480e9);
        assert_eq!(frequency_hz_from_channel_index(0).unwrap(), 2.404e9);
        assert_eq!(frequency_hz_from_channel_index(10).unwrap(), 2.424e9);
        assert_eq!(frequency_hz_from_channel_index(11).unwrap(), 2.428e9);
        assert_eq!(frequency_hz_from_channel_index(36).unwrap(), 2.478e9);
        assert!(matches!(
            frequency_hz_from_channel_index(40),
            Err(BleParseError::InvalidChannel(40))
        ));
    }
}
