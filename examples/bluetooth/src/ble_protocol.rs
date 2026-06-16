use std::error::Error;
use std::fmt;

pub use crate::ble_ad::format_ad_structures;
pub use crate::ble_ad::parse_ad_structures;
pub use crate::ble_phy::BLE_ACCESS_ADDRESS;
pub use crate::ble_phy::BLE_ADV_CRC_INIT;
pub use crate::ble_phy::BLE_MAX_ADV_PAYLOAD_LEN;
pub use crate::ble_phy::apply_whitening;
pub use crate::ble_phy::crc24_ble;
pub use crate::ble_phy::frequency_hz_from_channel_index;
pub use crate::ble_phy::generate_ble_packet_bits;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleAdvPduType {
    AdvInd,
    AdvDirectInd,
    AdvNonconnInd,
    ScanReq,
    ScanRsp,
    ConnectInd,
    AdvScanInd,
    AdvExtInd,
    Reserved(u8),
}

impl BleAdvPduType {
    fn from_header(header: u8) -> Self {
        match header & 0x0f {
            0 => Self::AdvInd,
            1 => Self::AdvDirectInd,
            2 => Self::AdvNonconnInd,
            3 => Self::ScanReq,
            4 => Self::ScanRsp,
            5 => Self::ConnectInd,
            6 => Self::AdvScanInd,
            7 => Self::AdvExtInd,
            x => Self::Reserved(x),
        }
    }
}

impl fmt::Display for BleAdvPduType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AdvInd => write!(f, "ADV_IND"),
            Self::AdvDirectInd => write!(f, "ADV_DIRECT_IND"),
            Self::AdvNonconnInd => write!(f, "ADV_NONCONN_IND"),
            Self::ScanReq => write!(f, "SCAN_REQ"),
            Self::ScanRsp => write!(f, "SCAN_RSP"),
            Self::ConnectInd => write!(f, "CONNECT_IND"),
            Self::AdvScanInd => write!(f, "ADV_SCAN_IND"),
            Self::AdvExtInd => write!(f, "ADV_EXT_IND"),
            Self::Reserved(x) => write!(f, "RESERVED({x})"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlePacket {
    pub access_address: u32,
    pub channel_index: u8,
    pub pdu_type: BleAdvPduType,
    pub tx_add: bool,
    pub rx_add: bool,
    pub payload_len: usize,
    pub payload: Vec<u8>,
    pub pdu: Vec<u8>,
    pub crc: u32,
    pub crc_valid: bool,
}

impl BlePacket {
    pub fn advertiser_address(&self) -> Option<[u8; 6]> {
        match self.pdu_type {
            BleAdvPduType::AdvInd
            | BleAdvPduType::AdvDirectInd
            | BleAdvPduType::AdvNonconnInd
            | BleAdvPduType::ScanRsp
            | BleAdvPduType::AdvScanInd => self.payload.get(0..6).map(slice_to_addr),
            BleAdvPduType::ScanReq | BleAdvPduType::ConnectInd => {
                self.payload.get(6..12).map(slice_to_addr)
            }
            BleAdvPduType::AdvExtInd | BleAdvPduType::Reserved(_) => None,
        }
    }

    pub fn scanner_address(&self) -> Option<[u8; 6]> {
        match self.pdu_type {
            BleAdvPduType::ScanReq => self.payload.get(0..6).map(slice_to_addr),
            _ => None,
        }
    }

    pub fn initiator_address(&self) -> Option<[u8; 6]> {
        match self.pdu_type {
            BleAdvPduType::ConnectInd => self.payload.get(0..6).map(slice_to_addr),
            _ => None,
        }
    }

    pub fn target_address(&self) -> Option<[u8; 6]> {
        match self.pdu_type {
            BleAdvPduType::AdvDirectInd => self.payload.get(6..12).map(slice_to_addr),
            _ => None,
        }
    }

    pub fn advertising_data(&self) -> Option<&[u8]> {
        match self.pdu_type {
            BleAdvPduType::AdvInd
            | BleAdvPduType::AdvNonconnInd
            | BleAdvPduType::ScanRsp
            | BleAdvPduType::AdvScanInd => self.payload.get(6..),
            BleAdvPduType::AdvDirectInd
            | BleAdvPduType::ScanReq
            | BleAdvPduType::ConnectInd
            | BleAdvPduType::AdvExtInd
            | BleAdvPduType::Reserved(_) => None,
        }
    }

    pub fn extended_advertising_header(&self) -> Option<BleExtAdvHeader> {
        if self.pdu_type != BleAdvPduType::AdvExtInd {
            return None;
        }

        BleExtAdvHeader::parse(&self.payload)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleExtAdvMode {
    NonConnectableNonScannable,
    Connectable,
    Scannable,
    Reserved(u8),
}

impl fmt::Display for BleExtAdvMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonConnectableNonScannable => write!(f, "nonconn_nonscan"),
            Self::Connectable => write!(f, "connectable"),
            Self::Scannable => write!(f, "scannable"),
            Self::Reserved(x) => write!(f, "reserved({x})"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleExtAdvPhy {
    Le1M,
    Le2M,
    LeCoded,
    Reserved(u8),
}

impl fmt::Display for BleExtAdvPhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Le1M => write!(f, "1M"),
            Self::Le2M => write!(f, "2M"),
            Self::LeCoded => write!(f, "coded"),
            Self::Reserved(x) => write!(f, "reserved({x})"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleAuxPtr {
    pub channel: u8,
    pub offset_usec: u32,
    pub phy: BleExtAdvPhy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BleExtAdvHeader {
    pub mode: BleExtAdvMode,
    pub adv_a: Option<[u8; 6]>,
    pub target_a: Option<[u8; 6]>,
    pub adi: Option<u16>,
    pub aux_ptr: Option<BleAuxPtr>,
    pub tx_power: Option<i8>,
}

impl BleExtAdvHeader {
    fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < 2 {
            return None;
        }

        let ext_header_len = (payload[0] & 0x3f) as usize;
        let mode = match (payload[0] >> 6) & 0x03 {
            0 => BleExtAdvMode::NonConnectableNonScannable,
            1 => BleExtAdvMode::Connectable,
            2 => BleExtAdvMode::Scannable,
            x => BleExtAdvMode::Reserved(x),
        };

        if ext_header_len < 1 || payload.len() < 1 + ext_header_len {
            return None;
        }

        let flags = payload[1];
        let end = 1 + ext_header_len;
        let mut pos = 2usize;

        let adv_a = if flags & 0x01 != 0 {
            read_addr(payload, &mut pos, end)?
        } else {
            None
        };

        let target_a = if flags & 0x02 != 0 {
            read_addr(payload, &mut pos, end)?
        } else {
            None
        };

        if flags & 0x04 != 0 {
            skip_bytes(&mut pos, end, 1)?;
        }

        let adi = if flags & 0x08 != 0 {
            let bytes = read_bytes(payload, &mut pos, end, 2)?;
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        } else {
            None
        };

        let aux_ptr = if flags & 0x10 != 0 {
            let bytes = read_bytes(payload, &mut pos, end, 3)?;
            parse_aux_ptr(bytes)?
        } else {
            None
        };

        if flags & 0x20 != 0 {
            skip_bytes(&mut pos, end, 18)?;
        }

        let tx_power = if flags & 0x40 != 0 {
            let bytes = read_bytes(payload, &mut pos, end, 1)?;
            Some(bytes[0] as i8)
        } else {
            None
        };

        Some(Self {
            mode,
            adv_a,
            target_a,
            adi,
            aux_ptr,
            tx_power,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BleParseError {
    InvalidChannel(u8),
    TooShort { actual: usize },
    PayloadTooLong { len: usize },
    Truncated { needed: usize, actual: usize },
    InvalidCrc { expected: u32, received: u32 },
}

impl fmt::Display for BleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChannel(ch) => write!(f, "invalid BLE channel index {ch}"),
            Self::TooShort { actual } => write!(f, "BLE packet too short ({actual} bytes)"),
            Self::PayloadTooLong { len } => {
                write!(f, "BLE advertising payload too long ({len} bytes)")
            }
            Self::Truncated { needed, actual } => {
                write!(f, "truncated BLE packet: need {needed} bytes, got {actual}")
            }
            Self::InvalidCrc { expected, received } => write!(
                f,
                "invalid BLE CRC: expected 0x{expected:06X}, received 0x{received:06X}"
            ),
        }
    }
}

impl Error for BleParseError {}

pub fn parse_advertising_pdu(
    channel_index: u8,
    whitened_pdu_crc: &[u8],
) -> Result<BlePacket, BleParseError> {
    if whitened_pdu_crc.len() < 5 {
        return Err(BleParseError::TooShort {
            actual: whitened_pdu_crc.len(),
        });
    }

    let pdu_crc = apply_whitening(channel_index, whitened_pdu_crc)?;
    let payload_len = (pdu_crc[1] & 0x3f) as usize;
    if payload_len > BLE_MAX_ADV_PAYLOAD_LEN {
        return Err(BleParseError::PayloadTooLong { len: payload_len });
    }

    let needed = 2 + payload_len + 3;
    if pdu_crc.len() < needed {
        return Err(BleParseError::Truncated {
            needed,
            actual: pdu_crc.len(),
        });
    }

    let pdu = pdu_crc[0..2 + payload_len].to_vec();
    let crc_bytes = &pdu_crc[2 + payload_len..needed];
    let received_crc =
        crc_bytes[0] as u32 | ((crc_bytes[1] as u32) << 8) | ((crc_bytes[2] as u32) << 16);
    let expected_crc = crc24_ble(&pdu, BLE_ADV_CRC_INIT);

    if expected_crc != received_crc {
        return Err(BleParseError::InvalidCrc {
            expected: expected_crc,
            received: received_crc,
        });
    }

    Ok(BlePacket {
        access_address: BLE_ACCESS_ADDRESS,
        channel_index,
        pdu_type: BleAdvPduType::from_header(pdu[0]),
        tx_add: pdu[0] & 0x40 != 0,
        rx_add: pdu[0] & 0x80 != 0,
        payload_len,
        payload: pdu[2..].to_vec(),
        pdu,
        crc: received_crc,
        crc_valid: true,
    })
}

pub fn format_addr(addr: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
    )
}

pub fn format_packet_summary(packet: &BlePacket) -> String {
    let mut fields = vec![
        format!("ch={}", packet.channel_index),
        format!("type={}", packet.pdu_type),
        format!("len={}", packet.payload_len),
    ];

    append_address_fields(packet, &mut fields);

    if let Some(ad_data) = packet.advertising_data() {
        match parse_ad_structures(ad_data) {
            Ok(ad_structures) if !ad_structures.is_empty() => {
                fields.push(format!("ad=[{}]", format_ad_structures(&ad_structures)));
            }
            Ok(_) => {}
            Err(err) => fields.push(format!("ad_error={err}")),
        }
    }

    if let Some(ext_header) = packet.extended_advertising_header() {
        fields.push(format!("ext=[{}]", format_ext_adv_header(&ext_header)));
    }

    fields.join(" ")
}

fn append_address_fields(packet: &BlePacket, fields: &mut Vec<String>) {
    match packet.pdu_type {
        BleAdvPduType::AdvInd
        | BleAdvPduType::AdvNonconnInd
        | BleAdvPduType::ScanRsp
        | BleAdvPduType::AdvScanInd => {
            if let Some(addr) = packet.advertiser_address() {
                fields.push(format!("adv_a={}", format_addr(&addr)));
            }
        }
        BleAdvPduType::AdvDirectInd => {
            if let Some(addr) = packet.advertiser_address() {
                fields.push(format!("adv_a={}", format_addr(&addr)));
            }
            if let Some(addr) = packet.target_address() {
                fields.push(format!("target_a={}", format_addr(&addr)));
            }
        }
        BleAdvPduType::ScanReq => {
            if let Some(addr) = packet.scanner_address() {
                fields.push(format!("scan_a={}", format_addr(&addr)));
            }
            if let Some(addr) = packet.advertiser_address() {
                fields.push(format!("adv_a={}", format_addr(&addr)));
            }
        }
        BleAdvPduType::ConnectInd => {
            if let Some(addr) = packet.initiator_address() {
                fields.push(format!("init_a={}", format_addr(&addr)));
            }
            if let Some(addr) = packet.advertiser_address() {
                fields.push(format!("adv_a={}", format_addr(&addr)));
            }
        }
        BleAdvPduType::AdvExtInd => {
            if let Some(ext_header) = packet.extended_advertising_header() {
                if let Some(addr) = ext_header.adv_a {
                    fields.push(format!("adv_a={}", format_addr(&addr)));
                }
                if let Some(addr) = ext_header.target_a {
                    fields.push(format!("target_a={}", format_addr(&addr)));
                }
            }
        }
        BleAdvPduType::Reserved(_) => {}
    }
}

fn format_ext_adv_header(header: &BleExtAdvHeader) -> String {
    let mut fields = vec![format!("mode={}", header.mode)];

    if let Some(adi) = header.adi {
        fields.push(format!("adi=0x{adi:04X}"));
    }

    if let Some(aux_ptr) = header.aux_ptr {
        fields.push(format!(
            "aux_ch={} aux_offset={}us aux_phy={}",
            aux_ptr.channel, aux_ptr.offset_usec, aux_ptr.phy
        ));
    }

    if let Some(tx_power) = header.tx_power {
        fields.push(format!("tx_power={tx_power}dBm"));
    }

    fields.join(" ")
}

fn read_addr(payload: &[u8], pos: &mut usize, end: usize) -> Option<Option<[u8; 6]>> {
    let bytes = read_bytes(payload, pos, end, 6)?;
    let mut addr = [0u8; 6];
    addr.copy_from_slice(bytes);
    Some(Some(addr))
}

fn read_bytes<'a>(
    payload: &'a [u8],
    pos: &mut usize,
    end: usize,
    count: usize,
) -> Option<&'a [u8]> {
    if *pos + count > end || *pos + count > payload.len() {
        return None;
    }

    let bytes = &payload[*pos..*pos + count];
    *pos += count;
    Some(bytes)
}

fn skip_bytes(pos: &mut usize, end: usize, count: usize) -> Option<()> {
    if *pos + count > end {
        return None;
    }

    *pos += count;
    Some(())
}

fn parse_aux_ptr(bytes: &[u8]) -> Option<Option<BleAuxPtr>> {
    let raw = bytes[0] as u32 | ((bytes[1] as u32) << 8) | ((bytes[2] as u32) << 16);
    let channel = (raw & 0x3f) as u8;
    if channel > 39 {
        return None;
    }

    let offset_units = (raw >> 7) & 0x01;
    let aux_offset = (raw >> 8) & 0x1fff;
    let phy = match (raw >> 21) & 0x07 {
        0 => BleExtAdvPhy::Le1M,
        1 => BleExtAdvPhy::Le2M,
        2 => BleExtAdvPhy::LeCoded,
        x => BleExtAdvPhy::Reserved(x as u8),
    };
    let offset_usec = aux_offset * if offset_units == 0 { 30 } else { 300 };

    Some(Some(BleAuxPtr {
        channel,
        offset_usec,
        phy,
    }))
}

fn slice_to_addr(slice: &[u8]) -> [u8; 6] {
    let mut addr = [0u8; 6];
    addr.copy_from_slice(slice);
    addr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_phy::apply_whitening;
    use crate::ble_phy::build_advertising_pdu_crc;

    #[test]
    fn parses_valid_advertising_packet() {
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let adv_data = [0x02, 0x01, 0x06];
        let whitened =
            build_advertising_pdu_crc(37, BleAdvPduType::AdvNonconnInd, &adv_a, &adv_data);
        let packet = parse_advertising_pdu(37, &whitened).unwrap();

        assert_eq!(packet.access_address, BLE_ACCESS_ADDRESS);
        assert_eq!(packet.channel_index, 37);
        assert_eq!(packet.pdu_type, BleAdvPduType::AdvNonconnInd);
        assert_eq!(packet.payload_len, 9);
        assert_eq!(packet.advertiser_address(), Some(adv_a));
        assert_eq!(packet.payload[6..], adv_data);
        assert!(packet.crc_valid);
    }

    #[test]
    fn packet_summary_includes_advertising_data() {
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let adv_data = [0x02, 0x01, 0x06, 0x05, 0x09, b'F', b'S', b'D', b'R'];
        let whitened = build_advertising_pdu_crc(37, BleAdvPduType::AdvInd, &adv_a, &adv_data);
        let packet = parse_advertising_pdu(37, &whitened).unwrap();

        assert_eq!(
            format_packet_summary(&packet),
            "ch=37 type=ADV_IND len=15 adv_a=66:55:44:33:22:11 ad=[flags=0x06, name=\"FSDR\"]"
        );
    }

    #[test]
    fn packet_summary_labels_scan_request_addresses() {
        let scan_a = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let whitened = build_advertising_pdu_crc(37, BleAdvPduType::ScanReq, &scan_a, &adv_a);
        let packet = parse_advertising_pdu(37, &whitened).unwrap();

        assert_eq!(packet.scanner_address(), Some(scan_a));
        assert_eq!(packet.advertiser_address(), Some(adv_a));
        assert_eq!(
            format_packet_summary(&packet),
            "ch=37 type=SCAN_REQ len=12 scan_a=FF:EE:DD:CC:BB:AA adv_a=66:55:44:33:22:11"
        );
    }

    #[test]
    fn rejects_bad_crc() {
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let adv_data = [0x02, 0x01, 0x06];
        let mut whitened =
            build_advertising_pdu_crc(37, BleAdvPduType::AdvNonconnInd, &adv_a, &adv_data);
        let last = whitened.len() - 1;
        whitened[last] ^= 0x01;

        assert!(matches!(
            parse_advertising_pdu(37, &whitened),
            Err(BleParseError::InvalidCrc { .. })
        ));
    }

    #[test]
    fn packet_summary_includes_extended_advertising_header() {
        let adv_a = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let aux_ptr = build_aux_ptr(5, 100, false, BleExtAdvPhy::Le1M);
        let payload = [
            11,   // extended header length: flags + AdvA + AuxPtr + TxPower
            0x51, // AdvA + AuxPtr + TxPower
            adv_a[0], adv_a[1], adv_a[2], adv_a[3], adv_a[4], adv_a[5], aux_ptr[0], aux_ptr[1],
            aux_ptr[2], 0xf8,
        ];
        let whitened = build_raw_pdu_crc(37, BleAdvPduType::AdvExtInd, &payload);
        let packet = parse_advertising_pdu(37, &whitened).unwrap();

        assert_eq!(
            format_packet_summary(&packet),
            "ch=37 type=ADV_EXT_IND len=12 adv_a=FF:EE:DD:CC:BB:AA ext=[mode=nonconn_nonscan aux_ch=5 aux_offset=3000us aux_phy=1M tx_power=-8dBm]"
        );
    }

    fn build_raw_pdu_crc(channel_index: u8, pdu_type: BleAdvPduType, payload: &[u8]) -> Vec<u8> {
        let pdu_type = match pdu_type {
            BleAdvPduType::AdvInd => 0,
            BleAdvPduType::AdvDirectInd => 1,
            BleAdvPduType::AdvNonconnInd => 2,
            BleAdvPduType::ScanReq => 3,
            BleAdvPduType::ScanRsp => 4,
            BleAdvPduType::ConnectInd => 5,
            BleAdvPduType::AdvScanInd => 6,
            BleAdvPduType::AdvExtInd => 7,
            BleAdvPduType::Reserved(x) => x,
        };

        let mut pdu = vec![pdu_type, payload.len() as u8];
        pdu.extend_from_slice(payload);
        let crc = crc24_ble(&pdu, BLE_ADV_CRC_INIT);
        pdu.push((crc & 0xff) as u8);
        pdu.push(((crc >> 8) & 0xff) as u8);
        pdu.push(((crc >> 16) & 0xff) as u8);

        apply_whitening(channel_index, &pdu).unwrap()
    }

    fn build_aux_ptr(
        channel: u8,
        aux_offset: u32,
        offset_units_300us: bool,
        phy: BleExtAdvPhy,
    ) -> [u8; 3] {
        let phy = match phy {
            BleExtAdvPhy::Le1M => 0,
            BleExtAdvPhy::Le2M => 1,
            BleExtAdvPhy::LeCoded => 2,
            BleExtAdvPhy::Reserved(x) => x as u32,
        };
        let raw = channel as u32
            | ((offset_units_300us as u32) << 7)
            | ((aux_offset & 0x1fff) << 8)
            | (phy << 21);

        [raw as u8, (raw >> 8) as u8, (raw >> 16) as u8]
    }
}
