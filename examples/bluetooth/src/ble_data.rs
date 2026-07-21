use std::error::Error;
use std::fmt;

use crate::ble_phy::BlePhy;
use crate::ble_phy::apply_whitening;
use crate::ble_phy::crc24_ble;

pub(crate) const BLE_MAX_DATA_PAYLOAD_LEN: usize = 251;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BleDataLlid {
    Reserved,
    ContinuationOrEmpty,
    StartOrComplete,
    Control,
}

impl BleDataLlid {
    fn from_header(header: u8) -> Self {
        match header & 0x03 {
            0 => Self::Reserved,
            1 => Self::ContinuationOrEmpty,
            2 => Self::StartOrComplete,
            _ => Self::Control,
        }
    }
}

impl fmt::Display for BleDataLlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reserved => write!(f, "reserved"),
            Self::ContinuationOrEmpty => write!(f, "continuation/empty"),
            Self::StartOrComplete => write!(f, "start/complete"),
            Self::Control => write!(f, "control"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BleDataHeader {
    pub(crate) llid: BleDataLlid,
    pub(crate) nesn: bool,
    pub(crate) sn: bool,
    pub(crate) more_data: bool,
    pub(crate) cte_info_present: bool,
    pub(crate) payload_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BleDataPacket {
    pub(crate) access_address: u32,
    pub(crate) channel_index: u8,
    pub(crate) phy: BlePhy,
    pub(crate) header: BleDataHeader,
    pub(crate) payload: Vec<u8>,
    pub(crate) pdu: Vec<u8>,
    pub(crate) crc: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BleDataParseError {
    TooShort { actual: usize },
    PayloadTooLong { len: usize },
    Truncated { needed: usize, actual: usize },
    InvalidCrc { expected: u32, received: u32 },
    InvalidChannel(u8),
}

impl fmt::Display for BleDataParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(f, "data PDU is too short: {actual} bytes"),
            Self::PayloadTooLong { len } => {
                write!(
                    f,
                    "data payload length {len} exceeds {BLE_MAX_DATA_PAYLOAD_LEN}"
                )
            }
            Self::Truncated { needed, actual } => {
                write!(f, "truncated data PDU: need {needed} bytes, got {actual}")
            }
            Self::InvalidCrc { expected, received } => write!(
                f,
                "invalid BLE data CRC: expected 0x{expected:06X}, received 0x{received:06X}"
            ),
            Self::InvalidChannel(channel) => write!(f, "invalid BLE channel index {channel}"),
        }
    }
}

impl Error for BleDataParseError {}

pub(crate) fn parse_data_pdu(
    channel_index: u8,
    phy: BlePhy,
    access_address: u32,
    crc_init: u32,
    whitened_pdu_crc: &[u8],
) -> Result<BleDataPacket, BleDataParseError> {
    if whitened_pdu_crc.len() < 5 {
        return Err(BleDataParseError::TooShort {
            actual: whitened_pdu_crc.len(),
        });
    }

    let pdu_crc = apply_whitening(channel_index, whitened_pdu_crc)
        .map_err(|_| BleDataParseError::InvalidChannel(channel_index))?;
    let payload_len = pdu_crc[1] as usize;
    if payload_len > BLE_MAX_DATA_PAYLOAD_LEN {
        return Err(BleDataParseError::PayloadTooLong { len: payload_len });
    }

    let needed = 2 + payload_len + 3;
    if pdu_crc.len() < needed {
        return Err(BleDataParseError::Truncated {
            needed,
            actual: pdu_crc.len(),
        });
    }

    let pdu = pdu_crc[..2 + payload_len].to_vec();
    let crc_bytes = &pdu_crc[2 + payload_len..needed];
    let received_crc =
        crc_bytes[0] as u32 | ((crc_bytes[1] as u32) << 8) | ((crc_bytes[2] as u32) << 16);
    let expected_crc = crc24_ble(&pdu, crc_init);
    if expected_crc != received_crc {
        return Err(BleDataParseError::InvalidCrc {
            expected: expected_crc,
            received: received_crc,
        });
    }

    let header_byte = pdu[0];
    Ok(BleDataPacket {
        access_address,
        channel_index,
        phy,
        header: BleDataHeader {
            llid: BleDataLlid::from_header(header_byte),
            nesn: header_byte & 0x04 != 0,
            sn: header_byte & 0x08 != 0,
            more_data: header_byte & 0x10 != 0,
            cte_info_present: header_byte & 0x20 != 0,
            payload_len,
        },
        payload: pdu[2..].to_vec(),
        pdu,
        crc: received_crc,
    })
}

pub(crate) fn format_data_packet_summary(packet: &BleDataPacket) -> String {
    format!(
        "ch={} phy={} pdu=DATA aa=0x{:08X} llid={} payload_len={} nesn={} sn={} md={}",
        packet.channel_index,
        packet.phy.short_name(),
        packet.access_address,
        packet.header.llid,
        packet.header.payload_len,
        u8::from(packet.header.nesn),
        u8::from(packet.header.sn),
        u8::from(packet.header.more_data),
    )
}

#[cfg(test)]
pub(crate) fn build_data_pdu_crc(
    channel_index: u8,
    crc_init: u32,
    header: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut pdu = Vec::with_capacity(2 + payload.len() + 3);
    pdu.push(header);
    pdu.push(payload.len() as u8);
    pdu.extend_from_slice(payload);
    let crc = crc24_ble(&pdu, crc_init);
    pdu.push((crc & 0xff) as u8);
    pdu.push(((crc >> 8) & 0xff) as u8);
    pdu.push(((crc >> 16) & 0xff) as u8);
    apply_whitening(channel_index, &pdu).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connection_specific_data_pdu() {
        let access_address = 0x1234_5678;
        let crc_init = 0x00ab_cdef;
        let payload = [0x03, 0x00, 0x04, 0x00];
        let whitened = build_data_pdu_crc(0, crc_init, 0x1e, &payload);

        let packet = parse_data_pdu(0, BlePhy::Le1M, access_address, crc_init, &whitened).unwrap();

        assert_eq!(packet.access_address, access_address);
        assert_eq!(packet.header.llid, BleDataLlid::StartOrComplete);
        assert!(packet.header.nesn);
        assert!(packet.header.sn);
        assert!(packet.header.more_data);
        assert_eq!(packet.payload, payload);
        assert_eq!(
            format_data_packet_summary(&packet),
            "ch=0 phy=1M pdu=DATA aa=0x12345678 llid=start/complete payload_len=4 nesn=1 sn=1 md=1"
        );
    }

    #[test]
    fn rejects_wrong_connection_crc_init() {
        let whitened = build_data_pdu_crc(12, 0x0012_3456, 0x01, &[]);

        assert!(matches!(
            parse_data_pdu(12, BlePhy::Le1M, 0x8765_4321, 0x0065_4321, &whitened),
            Err(BleDataParseError::InvalidCrc { .. })
        ));
    }
}
