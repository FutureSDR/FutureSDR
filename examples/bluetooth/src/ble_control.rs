use std::error::Error;
use std::fmt;

use crate::ble_data::BleDataLlid;
use crate::ble_data::BleDataPacket;

const LL_CONNECTION_UPDATE_IND: u8 = 0x00;
const LL_CHANNEL_MAP_IND: u8 = 0x01;
const LL_TERMINATE_IND: u8 = 0x02;
const LL_START_ENC_REQ: u8 = 0x05;
const LL_START_ENC_RSP: u8 = 0x06;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionUpdateInd {
    pub(crate) window_size_units: u8,
    pub(crate) window_offset_units: u16,
    pub(crate) interval_units: u16,
    pub(crate) latency: u16,
    pub(crate) timeout_units: u16,
    pub(crate) instant: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChannelMapInd {
    pub(crate) channel_map: [u8; 5],
    pub(crate) instant: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BleControlPdu {
    ConnectionUpdateInd(ConnectionUpdateInd),
    ChannelMapInd(ChannelMapInd),
    TerminateInd { error_code: u8 },
    StartEncryptionReq,
    StartEncryptionRsp,
    Unknown { opcode: u8 },
}

impl fmt::Display for BleControlPdu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionUpdateInd(update) => write!(
                f,
                "LL_CONNECTION_UPDATE_IND(interval={:.2}ms,latency={},timeout={}ms,instant={})",
                update.interval_units as f32 * 1.25,
                update.latency,
                update.timeout_units as u32 * 10,
                update.instant,
            ),
            Self::ChannelMapInd(update) => write!(
                f,
                "LL_CHANNEL_MAP_IND(map={:02X?},instant={})",
                update.channel_map, update.instant,
            ),
            Self::TerminateInd { error_code } => {
                write!(f, "LL_TERMINATE_IND(error=0x{error_code:02X})")
            }
            Self::StartEncryptionReq => f.write_str("LL_START_ENC_REQ"),
            Self::StartEncryptionRsp => f.write_str("LL_START_ENC_RSP"),
            Self::Unknown { opcode } => write!(f, "LL_CONTROL(opcode=0x{opcode:02X})"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BleControlParseError {
    MissingOpcode,
    InvalidLength {
        opcode: u8,
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for BleControlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOpcode => f.write_str("LL Control PDU is missing its opcode"),
            Self::InvalidLength {
                opcode,
                expected,
                actual,
            } => write!(
                f,
                "LL Control opcode 0x{opcode:02X} expects {expected} payload bytes, got {actual}",
            ),
        }
    }
}

impl Error for BleControlParseError {}

pub(crate) fn parse_control_pdu(
    packet: &BleDataPacket,
) -> Result<Option<BleControlPdu>, BleControlParseError> {
    if packet.header.llid != BleDataLlid::Control {
        return Ok(None);
    }
    let opcode = *packet
        .payload
        .first()
        .ok_or(BleControlParseError::MissingOpcode)?;
    let control = match opcode {
        LL_CONNECTION_UPDATE_IND => {
            expect_len(opcode, &packet.payload, 12)?;
            BleControlPdu::ConnectionUpdateInd(ConnectionUpdateInd {
                window_size_units: packet.payload[1],
                window_offset_units: u16::from_le_bytes([packet.payload[2], packet.payload[3]]),
                interval_units: u16::from_le_bytes([packet.payload[4], packet.payload[5]]),
                latency: u16::from_le_bytes([packet.payload[6], packet.payload[7]]),
                timeout_units: u16::from_le_bytes([packet.payload[8], packet.payload[9]]),
                instant: u16::from_le_bytes([packet.payload[10], packet.payload[11]]),
            })
        }
        LL_CHANNEL_MAP_IND => {
            expect_len(opcode, &packet.payload, 8)?;
            let mut channel_map = [0u8; 5];
            channel_map.copy_from_slice(&packet.payload[1..6]);
            BleControlPdu::ChannelMapInd(ChannelMapInd {
                channel_map,
                instant: u16::from_le_bytes([packet.payload[6], packet.payload[7]]),
            })
        }
        LL_TERMINATE_IND => {
            expect_len(opcode, &packet.payload, 2)?;
            BleControlPdu::TerminateInd {
                error_code: packet.payload[1],
            }
        }
        LL_START_ENC_REQ => {
            expect_len(opcode, &packet.payload, 1)?;
            BleControlPdu::StartEncryptionReq
        }
        LL_START_ENC_RSP => {
            expect_len(opcode, &packet.payload, 1)?;
            BleControlPdu::StartEncryptionRsp
        }
        _ => BleControlPdu::Unknown { opcode },
    };
    Ok(Some(control))
}

fn expect_len(opcode: u8, payload: &[u8], expected: usize) -> Result<(), BleControlParseError> {
    if payload.len() == expected {
        Ok(())
    } else {
        Err(BleControlParseError::InvalidLength {
            opcode,
            expected,
            actual: payload.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;
    use crate::ble_phy::BlePhy;

    const AA: u32 = 0x1234_5678;
    const CRC_INIT: u32 = 0x00ab_cdef;

    #[test]
    fn parses_connection_update_ind() {
        let payload = [
            0x00, 0x02, 0x04, 0x00, 0x18, 0x00, 0x01, 0x00, 0xc8, 0x00, 0x2a, 0x00,
        ];
        let packet = control_packet(0, &payload);

        assert_eq!(
            parse_control_pdu(&packet).unwrap(),
            Some(BleControlPdu::ConnectionUpdateInd(ConnectionUpdateInd {
                window_size_units: 2,
                window_offset_units: 4,
                interval_units: 24,
                latency: 1,
                timeout_units: 200,
                instant: 42,
            }))
        );
    }

    #[test]
    fn parses_channel_map_and_terminate_indications() {
        let channel_map = control_packet(1, &[0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x34, 0x12]);
        let terminate = control_packet(1, &[0x02, 0x13]);

        assert_eq!(
            parse_control_pdu(&channel_map).unwrap(),
            Some(BleControlPdu::ChannelMapInd(ChannelMapInd {
                channel_map: [0x03, 0, 0, 0, 0],
                instant: 0x1234,
            }))
        );
        assert_eq!(
            parse_control_pdu(&terminate).unwrap(),
            Some(BleControlPdu::TerminateInd { error_code: 0x13 })
        );
    }

    #[test]
    fn rejects_malformed_known_control_pdu() {
        let packet = control_packet(0, &[0x01, 0x03]);

        assert!(matches!(
            parse_control_pdu(&packet),
            Err(BleControlParseError::InvalidLength {
                opcode: LL_CHANNEL_MAP_IND,
                expected: 8,
                actual: 2,
            })
        ));
    }

    fn control_packet(channel: u8, payload: &[u8]) -> BleDataPacket {
        let whitened = build_data_pdu_crc(channel, CRC_INIT, 0x03, payload);
        parse_data_pdu(channel, BlePhy::Le1M, AA, CRC_INIT, &whitened).unwrap()
    }
}
