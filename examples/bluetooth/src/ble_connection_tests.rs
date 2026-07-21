use super::*;
use crate::ble_data::build_data_pdu_crc;
use crate::ble_data::parse_data_pdu;
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
    assert_eq!(connection.channel_selector.used_channels().len(), 37);
    assert_eq!(connection.seen_count, 2);
}

#[test]
fn tracking_summary_separates_requests_from_connections() {
    let packet = connection_packet(37, 0xa1b2c3d4);
    let table = SharedConnectionTable::with_monitored_channels(&[0, 1, 37]);

    table.observe_packet(&packet);
    table.observe_packet(&packet);

    assert_eq!(
        table.tracking_summary(),
        ConnectionTrackingSummary {
            connect_ind_seen: 2,
            connections: 1,
            locked_connections: 0,
            searching_connections: 1,
            ..ConnectionTrackingSummary::default()
        }
    );
}

#[test]
fn ignores_non_connection_packets() {
    let mut table = ConnectionTable::default();

    table.observe_packet(&advertising_packet(37));

    assert_eq!(table.len(), 0);
}

#[test]
fn expires_stale_connections_and_refreshes_activity_from_data() {
    let sample_rate_hz = 2_000_000;
    let connect_sample = 100_000;
    let access_address = 0xa1b2c3d4;
    let mut table = ConnectionTable::with_monitored_channels(&[0, 1]);
    table.observe_packet_at(
        &connection_packet(37, access_address),
        connect_sample,
        sample_rate_hz,
    );
    let retention_ms = table.iter().next().unwrap().retention_ms();
    assert_eq!(retention_ms, 38_000);
    let retention_samples = sample_rate_hz * retention_ms / 1_000;

    let data_sample = 293_580;
    let data = data_packet(11, access_address, 0x01, &[]);
    table.observe_data_packet_at(&data, data_sample, sample_rate_hz);

    table.observe_packet_at(
        &advertising_packet(37),
        connect_sample + retention_samples + 1,
        sample_rate_hz,
    );
    assert_eq!(table.len(), 1);

    table.observe_packet_at(
        &advertising_packet(37),
        data_sample + retention_samples + 1,
        sample_rate_hz,
    );
    assert_eq!(table.len(), 0);
    assert_eq!(table.expired_connections, 1);
    assert_eq!(table.connect_ind_seen, 1);
}

#[test]
fn replaces_an_older_connection_attempt_for_the_same_devices() {
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;

    let mut table = ConnectionTable::default();

    table.observe_packet(&connection_packet(37, 0xa1b2c3d4));
    let data = build_data_pdu_crc(11, 0x123456, 0x01, &[]);
    let data = parse_data_pdu(11, BlePhy::Le1M, 0xa1b2c3d4, 0x123456, &data).unwrap();
    table.observe_data_packet_with_timing(&data, None);
    table.observe_packet(&connection_packet(37, 0x12345678));

    assert_eq!(table.len(), 1);
    let connection = table.iter().next().unwrap();
    assert_eq!(connection.access_address, 0x12345678);
    assert_eq!(connection.seen_count, 2);
    assert_eq!(connection.data_packet_count, 0);
    assert_eq!(table.tracked_data_packets, 1);
}

#[test]
fn locks_connection_event_timing_from_a_data_packet() {
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;

    let sample_rate_hz = 2_000_000;
    let connect_end_sample = 100_000;
    let access_address = 0xa1b2c3d4;
    let mut table = ConnectionTable::with_monitored_channels(&[11]);
    table.observe_packet_at(
        &connection_packet(37, access_address),
        connect_end_sample,
        sample_rate_hz,
    );

    let packet_start_sample = 293_500;
    let aa_end_sample = packet_start_sample + DATA_PACKET_PREFIX_SYMBOLS * 2;
    let whitened = build_data_pdu_crc(11, 0x123456, 0x01, &[]);
    let packet = parse_data_pdu(11, BlePhy::Le1M, access_address, 0x123456, &whitened).unwrap();

    let observation = table
        .observe_data_packet_at(&packet, aa_end_sample, sample_rate_hz)
        .unwrap();

    assert_eq!(observation.event_counter, 3);
    assert_eq!(observation.channel_index, 11);
    let connection = table.iter().next().unwrap();
    assert_eq!(connection.event_zero_anchor_sample, Some(113_500));
    assert_eq!(connection.last_event_index, Some(3));
    assert_eq!(connection.timing_misses.total(), 0);
    assert_eq!(connection.last_channel, 11);
    assert_eq!(connection.data_packet_count, 1);

    let plan = connection.follow_plan(2).unwrap();
    assert_eq!(plan.access_address, access_address);
    assert_eq!(plan.events[0].event_index, 4);
    assert_eq!(plan.events[0].event_counter, 4);
    assert_eq!(plan.events[0].sample_index, 353_500);
    assert_eq!(plan.events[1].event_index, 5);
    assert_eq!(plan.events[1].sample_index, 413_500);

    let matching = table.decode_contexts_for_window(23, 353_300, 353_700, 200);
    assert_eq!(matching.locked_checks, 1);
    assert_eq!(matching.window_matches, 1);
    assert_eq!(matching.contexts.len(), 1);
    let wrong_channel = table.decode_contexts_for_window(22, 353_300, 353_700, 200);
    assert_eq!(wrong_channel.locked_checks, 1);
    assert_eq!(wrong_channel.window_matches, 0);
    assert!(wrong_channel.contexts.is_empty());

    let older_packet_start_sample = 233_500;
    let older_aa_end_sample = older_packet_start_sample + DATA_PACKET_PREFIX_SYMBOLS * 2;
    let older_whitened = build_data_pdu_crc(36, 0x123456, 0x01, &[]);
    let older_packet =
        parse_data_pdu(36, BlePhy::Le1M, access_address, 0x123456, &older_whitened).unwrap();

    let older_observation = table
        .observe_data_packet_at(&older_packet, older_aa_end_sample, sample_rate_hz)
        .unwrap();

    assert_eq!(older_observation.event_counter, 2);
    assert_eq!(table.iter().next().unwrap().last_event_index, Some(3));
}

#[test]
fn overflow_disables_timing_gating_but_keeps_decode_context() {
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;

    let sample_rate_hz = 2_000_000;
    let access_address = 0xa1b2c3d4;
    let mut table = ConnectionTable::with_monitored_channels(&[11]);
    table.observe_packet_at(
        &connection_packet(37, access_address),
        100_000,
        sample_rate_hz,
    );

    let packet_start_sample = 293_500;
    let aa_end_sample = packet_start_sample + DATA_PACKET_PREFIX_SYMBOLS * 2;
    let whitened = build_data_pdu_crc(11, 0x123456, 0x01, &[]);
    let packet = parse_data_pdu(11, BlePhy::Le1M, access_address, 0x123456, &whitened).unwrap();
    assert!(
        table
            .observe_data_packet_at(&packet, aa_end_sample, sample_rate_hz)
            .is_some()
    );

    table.observe_overflow(7);
    table.observe_overflow(7);

    let selection = table.decode_contexts_for_window(22, 400_000, 401_000, 200);
    assert_eq!(selection.locked_checks, 0);
    assert_eq!(selection.searching_checks, 1);
    assert_eq!(selection.contexts.len(), 1);
    let connection = table.iter().next().unwrap();
    assert!(connection.timing_discontinuous);
    assert_eq!(connection.last_event_index, None);
    assert_eq!(table.overflow_events, 7);

    table.connections[0].timing_recovery = Some(TimingRecovery::new(None));
    table.observe_overflow(8);
    table.observe_overflow(8);
    assert_eq!(table.recovery_resets, 1);
}

#[test]
fn csa1_relocks_hopping_phase_after_overflow() {
    use crate::ble_data::build_data_pdu_crc;
    use crate::ble_data::parse_data_pdu;

    let sample_rate_hz = 2_000_000;
    let interval_samples = 60_000;
    let access_address = 0xa1b2c3d4;
    let mut table = ConnectionTable::with_monitored_channels(&[0, 1, 37]);
    table.observe_packet_at(
        &connection_packet(37, access_address),
        100_000,
        sample_rate_hz,
    );

    let initial_start = 293_500;
    let initial = build_data_pdu_crc(11, 0x123456, 0x01, &[]);
    let initial = parse_data_pdu(11, BlePhy::Le1M, access_address, 0x123456, &initial).unwrap();
    assert!(
        table
            .observe_data_packet_at(
                &initial,
                initial_start + DATA_PACKET_PREFIX_SYMBOLS * 2,
                sample_rate_hz,
            )
            .is_some()
    );
    table.observe_overflow(1);

    let recovery_counter = 10;
    let recovery_channel = table
        .iter()
        .next()
        .unwrap()
        .channel_selector
        .channel_for_event(recovery_counter);
    let recovery_start = 400_000;
    let recovery = build_data_pdu_crc(recovery_channel, 0x123456, 0x01, &[]);
    let recovery = parse_data_pdu(
        recovery_channel,
        BlePhy::Le1M,
        access_address,
        0x123456,
        &recovery,
    )
    .unwrap();
    let observation = table
        .observe_data_packet_at(
            &recovery,
            recovery_start + DATA_PACKET_PREFIX_SYMBOLS * 2,
            sample_rate_hz,
        )
        .unwrap();

    assert_eq!(observation.event_counter, recovery_counter);
    assert!(!observation.counter_exact);
    let connection = table.iter().next().unwrap();
    assert!(!connection.timing_discontinuous);
    assert_eq!(connection.relock_count, 1);
    assert_eq!(table.recovery_observations, 1);
    assert_eq!(table.relocks, 1);

    let next_counter = recovery_counter + 1;
    let next_channel = connection.channel_selector.channel_for_event(next_counter);
    let next_sample = recovery_start + interval_samples;
    let selection =
        table.decode_contexts_for_window(next_channel, next_sample - 100, next_sample + 100, 200);
    assert_eq!(selection.locked_checks, 1);
    assert_eq!(selection.window_matches, 1);
    assert_eq!(selection.contexts.len(), 1);
}

#[test]
fn applies_channel_map_at_the_instant() {
    let sample_rate_hz = 2_000_000;
    let access_address = 0xa1b2c3d4;
    let mut table = locked_connection(access_address, sample_rate_hz);
    let update = data_packet(
        11,
        access_address,
        0x03,
        &[0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00],
    );

    table.observe_data_packet_at(&update, 293_580, sample_rate_hz);
    assert_eq!(table.channel_map_updates_scheduled, 1);
    assert_eq!(table.channel_map_updates_applied, 0);
    let selection = table.decode_contexts_for_window(1, 413_400, 413_600, 0);
    assert_eq!(selection.locked_checks, 0);
    assert_eq!(selection.searching_checks, 1);
    assert_eq!(selection.contexts.len(), 1);

    let packet = data_packet(1, access_address, 0x01, &[]);
    let observation = table
        .observe_data_packet_at(&packet, 413_580, sample_rate_hz)
        .unwrap();

    assert_eq!(observation.event_counter, 5);
    assert_eq!(observation.channel_index, 1);
    assert_eq!(table.channel_map_updates_applied, 1);
    assert_eq!(
        table
            .iter()
            .next()
            .unwrap()
            .channel_selector
            .used_channels(),
        &[0, 1]
    );
    let selection = table.decode_contexts_for_window(0, 473_400, 473_600, 0);
    assert_eq!(selection.locked_checks, 1);
}

#[test]
fn connection_update_reanchors_from_the_new_transmit_window() {
    let sample_rate_hz = 2_000_000;
    let access_address = 0xa1b2c3d4;
    let mut table = locked_connection(access_address, sample_rate_hz);
    let update = data_packet(
        11,
        access_address,
        0x03,
        &[
            0x00, 0x02, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x05, 0x00,
        ],
    );

    table.observe_data_packet_at(&update, 293_580, sample_rate_hz);
    assert_eq!(table.connection_updates_scheduled, 1);

    let first_new_anchor = 416_000;
    let packet = data_packet(35, access_address, 0x01, &[]);
    let observation = table
        .observe_data_packet_at(
            &packet,
            first_new_anchor + DATA_PACKET_PREFIX_SYMBOLS * 2,
            sample_rate_hz,
        )
        .unwrap();

    assert_eq!(observation.event_counter, 5);
    let connection = table.iter().next().unwrap();
    assert_eq!(connection.interval_units, 40);
    assert_eq!(connection.event_zero_anchor_sample, Some(first_new_anchor));
    assert_eq!(
        connection.follow_plan(1).unwrap().events[0].sample_index,
        516_000
    );
    assert_eq!(table.connection_updates_applied, 1);
}

#[test]
fn terminate_ind_removes_the_connection() {
    let sample_rate_hz = 2_000_000;
    let access_address = 0xa1b2c3d4;
    let mut table = locked_connection(access_address, sample_rate_hz);
    let terminate = data_packet(23, access_address, 0x03, &[0x02, 0x13]);

    assert!(
        table
            .observe_data_packet_at(&terminate, 353_580, sample_rate_hz)
            .is_some()
    );

    assert_eq!(table.len(), 0);
    assert_eq!(table.terminated_connections, 1);
    assert_eq!(table.control_pdus, 1);
}

#[test]
fn encrypted_payload_is_not_interpreted_as_a_control_update() {
    let sample_rate_hz = 2_000_000;
    let access_address = 0xa1b2c3d4;
    let mut table = locked_connection(access_address, sample_rate_hz);
    let start_encryption = data_packet(23, access_address, 0x03, &[0x06]);
    table.observe_data_packet_at(&start_encryption, 353_580, sample_rate_hz);
    let ciphertext = data_packet(
        35,
        access_address,
        0x03,
        &[0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00],
    );
    table.observe_data_packet_at(&ciphertext, 413_580, sample_rate_hz);

    let connection = table.iter().next().unwrap();
    assert!(connection.encrypted);
    assert!(connection.pending_channel_map_update.is_none());
    assert_eq!(table.encryption_starts, 1);
    assert_eq!(table.channel_map_updates_scheduled, 0);
}

fn locked_connection(access_address: u32, sample_rate_hz: u64) -> ConnectionTable {
    let mut table = ConnectionTable::with_monitored_channels(&[0, 1, 11, 23, 35]);
    table.observe_packet_at(
        &connection_packet(37, access_address),
        100_000,
        sample_rate_hz,
    );
    let packet = data_packet(11, access_address, 0x01, &[]);
    assert!(
        table
            .observe_data_packet_at(&packet, 293_580, sample_rate_hz)
            .is_some()
    );
    table
}

fn data_packet(channel: u8, access_address: u32, header: u8, payload: &[u8]) -> BleDataPacket {
    let whitened = build_data_pdu_crc(channel, 0x123456, header, payload);
    parse_data_pdu(channel, BlePhy::Le1M, access_address, 0x123456, &whitened).unwrap()
}

fn advertising_packet(channel: u8) -> BlePacket {
    let adv_a = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let adv_data = [0x02, 0x01, 0x06];
    let whitened = build_advertising_pdu_crc(channel, BleAdvPduType::AdvInd, &adv_a, &adv_data);
    parse_advertising_pdu(channel, &whitened).unwrap()
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
