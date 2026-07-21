use crate::ble_data::BleDataPacket;
use crate::ble_protocol::BlePacket;
use crate::ble_protocol::BlePhy;

const RFTAP_VERSION: u16 = 3;
const RFTAP_PACKET_TYPE: u16 = 1;
const DLT_BLUETOOTH_LE_LL_WITH_PHDR: u32 = 256;

const LE_DEWHITENED: u16 = 0x0001;
const LE_CRC_CHECKED: u16 = 0x0400;
const LE_CRC_VALID: u16 = 0x0800;
const LE_PHY_1M: u16 = 0x0000;
const LE_PHY_2M: u16 = 0x4000;
const LE_PHY_CODED: u16 = 0x8000;

pub(crate) fn packet_to_udp_payload(packet: &BlePacket) -> Vec<u8> {
    build_udp_payload(
        packet.channel_index,
        packet.phy,
        packet.access_address,
        &packet.pdu,
        packet.crc,
    )
}

pub(crate) fn data_packet_to_udp_payload(packet: &BleDataPacket) -> Vec<u8> {
    build_udp_payload(
        packet.channel_index,
        packet.phy,
        packet.access_address,
        &packet.pdu,
        packet.crc,
    )
}

fn build_udp_payload(
    channel_index: u8,
    phy: BlePhy,
    access_address: u32,
    pdu: &[u8],
    crc: u32,
) -> Vec<u8> {
    let ble_packet_len = 4 + pdu.len() + 3;
    let mut payload = Vec::with_capacity(12 + 10 + ble_packet_len);

    payload.extend_from_slice(b"RFta");
    payload.extend_from_slice(&RFTAP_VERSION.to_le_bytes());
    payload.extend_from_slice(&RFTAP_PACKET_TYPE.to_le_bytes());
    payload.extend_from_slice(&DLT_BLUETOOTH_LE_LL_WITH_PHDR.to_le_bytes());

    payload.push(rf_channel(channel_index));
    payload.push(0);
    payload.push(0);
    payload.push(0);
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(
        &(LE_DEWHITENED | LE_CRC_CHECKED | LE_CRC_VALID | phy_flags(phy)).to_le_bytes(),
    );

    payload.extend_from_slice(&access_address.to_le_bytes());
    payload.extend_from_slice(pdu);
    payload.push((crc & 0xff) as u8);
    payload.push(((crc >> 8) & 0xff) as u8);
    payload.push(((crc >> 16) & 0xff) as u8);
    payload
}

fn phy_flags(phy: BlePhy) -> u16 {
    match phy {
        BlePhy::Le1M => LE_PHY_1M,
        BlePhy::Le2M => LE_PHY_2M,
        BlePhy::LeCoded => LE_PHY_CODED,
    }
}

fn rf_channel(channel_index: u8) -> u8 {
    match channel_index {
        37 => 0,
        38 => 12,
        39 => 39,
        0..=10 => channel_index + 1,
        11..=36 => channel_index + 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;
    use crate::ble_phy::build_advertising_pdu_crc;
    use crate::ble_protocol;
    use crate::ble_protocol::BLE_ACCESS_ADDRESS;
    use crate::ble_protocol::BleAdvPduType;

    #[test]
    fn builds_rftap_btle_udp_payload() {
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let adv_data = [0x02, 0x01, 0x06];
        let whitened =
            build_advertising_pdu_crc(37, BleAdvPduType::AdvNonconnInd, &adv_a, &adv_data);
        let packet = ble_protocol::parse_advertising_pdu(37, &whitened).unwrap();

        let payload = packet_to_udp_payload(&packet);

        assert_eq!(&payload[0..4], b"RFta");
        assert_eq!(&payload[4..6], &RFTAP_VERSION.to_le_bytes());
        assert_eq!(
            &payload[8..12],
            &DLT_BLUETOOTH_LE_LL_WITH_PHDR.to_le_bytes()
        );
        assert_eq!(payload[12], 0);
        assert_eq!(&payload[22..26], &BLE_ACCESS_ADDRESS.to_le_bytes());
        assert_eq!(&payload[26..26 + packet.pdu.len()], packet.pdu.as_slice());
        assert_eq!(payload.len(), 12 + 10 + 4 + packet.pdu.len() + 3);
    }

    #[test]
    fn maps_ble_channel_to_rf_channel() {
        assert_eq!(rf_channel(37), 0);
        assert_eq!(rf_channel(38), 12);
        assert_eq!(rf_channel(39), 39);
        assert_eq!(rf_channel(0), 1);
        assert_eq!(rf_channel(10), 11);
        assert_eq!(rf_channel(11), 13);
        assert_eq!(rf_channel(36), 38);
    }

    #[test]
    fn builds_rftap_payload_for_connection_data() {
        let access_address = 0x1234_5678;
        let crc_init = 0x00ab_cdef;
        let whitened = build_data_pdu_crc(0, crc_init, 0x02, &[0x01, 0x02]);
        let packet = parse_data_pdu(0, BlePhy::Le1M, access_address, crc_init, &whitened).unwrap();

        let payload = data_packet_to_udp_payload(&packet);

        assert_eq!(&payload[22..26], &access_address.to_le_bytes());
        assert_eq!(&payload[26..26 + packet.pdu.len()], packet.pdu.as_slice());
        assert_eq!(payload.len(), 12 + 10 + 4 + packet.pdu.len() + 3);
    }

    #[test]
    fn maps_ble_phy_to_rftap_flags() {
        assert_eq!(phy_flags(BlePhy::Le1M), LE_PHY_1M);
        assert_eq!(phy_flags(BlePhy::Le2M), LE_PHY_2M);
        assert_eq!(phy_flags(BlePhy::LeCoded), LE_PHY_CODED);
    }
}
