use crate::ble_protocol;
use crate::ble_protocol::BlePacket;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BleConnection {
    pub(crate) access_address: u32,
    pub(crate) crc_init: u32,
    pub(crate) initiator_address: [u8; 6],
    pub(crate) advertiser_address: [u8; 6],
    pub(crate) initiator_random: bool,
    pub(crate) advertiser_random: bool,
    pub(crate) interval_units: u16,
    pub(crate) latency: u16,
    pub(crate) timeout_units: u16,
    pub(crate) channel_map: [u8; 5],
    pub(crate) hop_increment: u8,
    pub(crate) sca: u8,
    pub(crate) first_channel: u8,
    pub(crate) last_channel: u8,
    pub(crate) seen_count: u64,
}

impl BleConnection {
    fn from_connect_ind(packet: &BlePacket) -> Option<Self> {
        let request = packet.connection_request()?;
        let initiator_address = packet.initiator_address()?;
        let advertiser_address = packet.advertiser_address()?;

        Some(Self {
            access_address: request.access_address,
            crc_init: request.crc_init,
            initiator_address,
            advertiser_address,
            initiator_random: packet.tx_add,
            advertiser_random: packet.rx_add,
            interval_units: request.interval_units,
            latency: request.latency,
            timeout_units: request.timeout_units,
            channel_map: request.channel_map,
            hop_increment: request.hop_increment,
            sca: request.sca,
            first_channel: packet.channel_index,
            last_channel: packet.channel_index,
            seen_count: 1,
        })
    }

    fn interval_ms(&self) -> f32 {
        self.interval_units as f32 * 1.25
    }

    fn timeout_ms(&self) -> u32 {
        self.timeout_units as u32 * 10
    }

    fn used_channel_count(&self) -> u32 {
        used_channel_count(&self.channel_map)
    }
}

#[derive(Default)]
pub(crate) struct ConnectionTable {
    connections: Vec<BleConnection>,
}

impl ConnectionTable {
    pub(crate) fn observe_packet(&mut self, packet: &BlePacket) {
        let Some(connection) = BleConnection::from_connect_ind(packet) else {
            return;
        };

        if let Some(existing) = self
            .connections
            .iter_mut()
            .find(|entry| entry.access_address == connection.access_address)
        {
            existing.last_channel = packet.channel_index;
            existing.seen_count += 1;
            return;
        }

        self.connections.push(connection);
    }

    pub(crate) fn len(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &BleConnection> {
        self.connections.iter()
    }
}

pub(crate) fn format_connection_summary(connection: &BleConnection) -> String {
    format!(
        "aa=0x{:08X} crc_init=0x{:06X} init_a={}{} adv_a={}{} interval={:.2}ms latency={} timeout={}ms hop={} sca={} channels={} first_ch={} last_ch={} seen={}",
        connection.access_address,
        connection.crc_init,
        ble_protocol::format_addr(&connection.initiator_address),
        addr_type_suffix(connection.initiator_random),
        ble_protocol::format_addr(&connection.advertiser_address),
        addr_type_suffix(connection.advertiser_random),
        connection.interval_ms(),
        connection.latency,
        connection.timeout_ms(),
        connection.hop_increment,
        connection.sca,
        connection.used_channel_count(),
        connection.first_channel,
        connection.last_channel,
        connection.seen_count,
    )
}

fn addr_type_suffix(random: bool) -> &'static str {
    if random { "(random)" } else { "(public)" }
}

fn used_channel_count(channel_map: &[u8; 5]) -> u32 {
    channel_map
        .iter()
        .enumerate()
        .map(|(byte_index, byte)| {
            if byte_index == 4 {
                (byte & 0x1f).count_ones()
            } else {
                byte.count_ones()
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_phy::BLE_ADV_CRC_INIT;
    use crate::ble_phy::apply_whitening;
    use crate::ble_phy::build_advertising_pdu_crc;
    use crate::ble_phy::crc24_ble;
    use crate::ble_protocol::BleAdvPduType;
    use crate::ble_protocol::parse_advertising_pdu;

    #[test]
    fn tracks_connection_request_once_per_access_address() {
        let packet = connection_packet(37, 0xa1b2c3d4);
        let mut table = ConnectionTable::default();

        table.observe_packet(&packet);
        table.observe_packet(&packet);

        assert_eq!(table.len(), 1);
        let connection = table.iter().next().unwrap();
        assert_eq!(connection.access_address, 0xa1b2c3d4);
        assert_eq!(connection.crc_init, 0x123456);
        assert_eq!(connection.hop_increment, 12);
        assert_eq!(connection.used_channel_count(), 37);
        assert_eq!(connection.seen_count, 2);
    }

    #[test]
    fn ignores_non_connection_packets() {
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let adv_data = [0x02, 0x01, 0x06];
        let whitened = build_advertising_pdu_crc(37, BleAdvPduType::AdvInd, &adv_a, &adv_data);
        let packet = parse_advertising_pdu(37, &whitened).unwrap();
        let mut table = ConnectionTable::default();

        table.observe_packet(&packet);

        assert_eq!(table.len(), 0);
    }

    fn connection_packet(channel: u8, access_address: u32) -> BlePacket {
        let init_a = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let mut payload = Vec::new();
        payload.extend_from_slice(&init_a);
        payload.extend_from_slice(&adv_a);
        payload.extend_from_slice(&access_address.to_le_bytes());
        payload.extend_from_slice(&[0x56, 0x34, 0x12]);
        payload.push(2);
        payload.extend_from_slice(&4u16.to_le_bytes());
        payload.extend_from_slice(&24u16.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&200u16.to_le_bytes());
        payload.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0x1f]);
        payload.push((3 << 5) | 12);

        let mut pdu = vec![5, payload.len() as u8];
        pdu.extend_from_slice(&payload);
        let crc = crc24_ble(&pdu, BLE_ADV_CRC_INIT);
        pdu.push((crc & 0xff) as u8);
        pdu.push(((crc >> 8) & 0xff) as u8);
        pdu.push(((crc >> 16) & 0xff) as u8);

        let whitened = apply_whitening(channel, &pdu).unwrap();
        parse_advertising_pdu(channel, &whitened).unwrap()
    }
}
