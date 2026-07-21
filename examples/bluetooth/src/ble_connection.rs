use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::ble_channel_selection::BleChannelSelectionAlgorithm;
use crate::ble_channel_selection::BleChannelSelector;
use crate::ble_connection_follower::ConnectionEventPlanner;
use crate::ble_connection_follower::ConnectionFollowPlan;
use crate::ble_connection_report::ConnectionTrackingSummary;
use crate::ble_control::BleControlPdu;
use crate::ble_control::ChannelMapInd;
use crate::ble_control::ConnectionUpdateInd;
use crate::ble_control::parse_control_pdu;
use crate::ble_data::BleDataPacket;
use crate::ble_phy::BlePhy;
use crate::ble_protocol;
use crate::ble_protocol::BlePacket;
use crate::ble_timing_recovery::TimingRecovery;

const DATA_PACKET_PREFIX_SYMBOLS: u64 = 40;
const MAX_TRACKED_CONNECTIONS: usize = 64;
const MIN_CONNECTION_RETENTION_MS: u64 = 5_000;
const TIMING_GUARD_SYMBOLS: u64 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionDecodeContext {
    pub(crate) access_address: u32,
    pub(crate) crc_init: u32,
    pub(crate) phy: BlePhy,
}

#[derive(Debug, Default)]
pub(crate) struct ConnectionDecodeSelection {
    pub(crate) contexts: Vec<ConnectionDecodeContext>,
    pub(crate) locked_checks: u64,
    pub(crate) window_matches: u64,
    pub(crate) searching_checks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionEventObservation {
    pub(crate) event_counter: u16,
    pub(crate) channel_index: u8,
    pub(crate) counter_exact: bool,
    event_index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingConnectionUpdate {
    parameters: ConnectionUpdateInd,
    instant_sample: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingChannelMapUpdate {
    parameters: ChannelMapInd,
    instant_sample: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ControlObservation {
    terminate: bool,
    parsed: u64,
    parse_errors: u64,
    unresolved_updates: u64,
    connection_updates_scheduled: u64,
    channel_map_updates_scheduled: u64,
    encryption_starts: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimingMissStats {
    sample_rate: u64,
    invalid_parameters: u64,
    before_window: u64,
    outside_window: u64,
    channel: u64,
}

impl TimingMissStats {
    fn total(self) -> u64 {
        self.sample_rate
            + self.invalid_parameters
            + self.before_window
            + self.outside_window
            + self.channel
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BleConnection {
    pub(crate) access_address: u32,
    pub(crate) crc_init: u32,
    pub(crate) phy: BlePhy,
    pub(crate) initiator_address: [u8; 6],
    pub(crate) advertiser_address: [u8; 6],
    pub(crate) initiator_random: bool,
    pub(crate) advertiser_random: bool,
    pub(crate) interval_units: u16,
    pub(crate) window_size_units: u8,
    pub(crate) window_offset_units: u16,
    pub(crate) latency: u16,
    pub(crate) timeout_units: u16,
    pub(crate) hop_increment: u8,
    pub(crate) sca: u8,
    pub(crate) first_channel: u8,
    pub(crate) last_channel: u8,
    pub(crate) seen_count: u64,
    pub(crate) data_packet_count: u64,
    channel_selector: BleChannelSelector,
    monitored_data_channels: Vec<u8>,
    sample_rate_hz: Option<u64>,
    connect_end_sample: Option<u64>,
    event_zero_anchor_sample: Option<u64>,
    last_event_index: Option<u64>,
    event_counter_offset: u16,
    event_counter_exact: bool,
    pending_connection_update: Option<PendingConnectionUpdate>,
    pending_channel_map_update: Option<PendingChannelMapUpdate>,
    encrypted: bool,
    connection_updates_applied: u64,
    channel_map_updates_applied: u64,
    timing_discontinuous: bool,
    timing_recovery: Option<TimingRecovery>,
    recovery_counter_hint: Option<u16>,
    relock_count: u64,
    timing_misses: TimingMissStats,
    last_activity_sample: Option<u64>,
    last_activity: Instant,
}

impl BleConnection {
    fn from_connect_ind(
        packet: &BlePacket,
        monitored_data_channels: &[u8],
        timing: Option<(u64, u64)>,
    ) -> Option<Self> {
        let request = packet.connection_request()?;
        let initiator_address = packet.initiator_address()?;
        let advertiser_address = packet.advertiser_address()?;

        let algorithm = if packet.channel_selection_2 {
            BleChannelSelectionAlgorithm::Csa2
        } else {
            BleChannelSelectionAlgorithm::Csa1
        };
        let channel_selector = BleChannelSelector::new(
            algorithm,
            request.access_address,
            request.hop_increment,
            request.channel_map,
        )
        .ok()?;

        Some(Self {
            access_address: request.access_address,
            crc_init: request.crc_init,
            phy: packet.phy,
            initiator_address,
            advertiser_address,
            initiator_random: packet.tx_add,
            advertiser_random: packet.rx_add,
            interval_units: request.interval_units,
            window_size_units: request.window_size_units,
            window_offset_units: request.window_offset_units,
            latency: request.latency,
            timeout_units: request.timeout_units,
            hop_increment: request.hop_increment,
            sca: request.sca,
            first_channel: packet.channel_index,
            last_channel: packet.channel_index,
            seen_count: 1,
            data_packet_count: 0,
            channel_selector,
            monitored_data_channels: monitored_data_channels.to_vec(),
            sample_rate_hz: timing.map(|(_, sample_rate_hz)| sample_rate_hz),
            connect_end_sample: timing.map(|(sample_index, _)| sample_index),
            event_zero_anchor_sample: None,
            last_event_index: None,
            event_counter_offset: 0,
            event_counter_exact: true,
            pending_connection_update: None,
            pending_channel_map_update: None,
            encrypted: false,
            connection_updates_applied: 0,
            channel_map_updates_applied: 0,
            timing_discontinuous: false,
            timing_recovery: None,
            recovery_counter_hint: None,
            relock_count: 0,
            timing_misses: TimingMissStats::default(),
            last_activity_sample: timing.map(|(sample_index, _)| sample_index),
            last_activity: Instant::now(),
        })
    }

    fn interval_ms(&self) -> f32 {
        self.interval_units as f32 * 1.25
    }

    fn timeout_ms(&self) -> u32 {
        self.timeout_units as u32 * 10
    }

    fn window_size_ms(&self) -> f32 {
        self.window_size_units as f32 * 1.25
    }

    fn window_offset_ms(&self) -> f32 {
        self.window_offset_units as f32 * 1.25
    }

    fn retention_ms(&self) -> u64 {
        let monitored_channels = self
            .channel_selector
            .used_channels()
            .iter()
            .filter(|channel| self.monitored_data_channels.contains(channel))
            .count();
        let coverage_scale = self
            .channel_selector
            .used_channels()
            .len()
            .div_ceil(monitored_channels.max(1)) as u64;
        (self.timeout_ms() as u64 * coverage_scale).max(MIN_CONNECTION_RETENTION_MS)
    }

    fn is_stale_at(&self, sample_index: u64, sample_rate_hz: u64) -> bool {
        if self.sample_rate_hz != Some(sample_rate_hz) {
            return false;
        }
        let Some(last_activity_sample) = self.last_activity_sample else {
            return false;
        };
        let Some(elapsed_samples) = sample_index.checked_sub(last_activity_sample) else {
            return false;
        };
        let retention_samples = sample_rate_hz
            .saturating_mul(self.retention_ms())
            .saturating_div(1_000);
        elapsed_samples > retention_samples
    }

    fn observe_data_timing(
        &mut self,
        packet: &BleDataPacket,
        aa_end_sample: u64,
        sample_rate_hz: u64,
    ) -> Option<ConnectionEventObservation> {
        if self.sample_rate_hz != Some(sample_rate_hz) {
            self.timing_misses.sample_rate += 1;
            return None;
        }

        let samples_per_symbol = sample_rate_hz / self.phy.symbol_rate_hz() as u64;
        if samples_per_symbol == 0 {
            self.timing_misses.invalid_parameters += 1;
            return None;
        }
        let packet_start_sample =
            aa_end_sample.saturating_sub(DATA_PACKET_PREFIX_SYMBOLS * samples_per_symbol);
        self.apply_pending_updates(packet_start_sample);
        let interval_samples = units_1_25_ms_to_samples(self.interval_units as u64, sample_rate_hz);
        if interval_samples == 0 {
            self.timing_misses.invalid_parameters += 1;
            return None;
        }
        if self.timing_discontinuous {
            return self.recover_data_timing(
                packet.channel_index,
                packet_start_sample,
                interval_samples,
            );
        }

        let event_index = if let Some(anchor) = self.event_zero_anchor_sample {
            let elapsed = packet_start_sample.saturating_sub(anchor);
            (elapsed + interval_samples / 2) / interval_samples
        } else {
            let connect_end = self.connect_end_sample?;
            let window_start = connect_end
                + units_1_25_ms_to_samples(1 + self.window_offset_units as u64, sample_rate_hz);
            if packet_start_sample < window_start {
                self.timing_misses.before_window += 1;
                return None;
            }
            let elapsed = packet_start_sample - window_start;
            let event_number = elapsed / interval_samples;
            let position_in_window = elapsed % interval_samples;
            let window_samples =
                units_1_25_ms_to_samples(self.window_size_units as u64, sample_rate_hz);
            let timing_guard = TIMING_GUARD_SYMBOLS * samples_per_symbol;
            if position_in_window > window_samples + timing_guard {
                self.timing_misses.outside_window += 1;
                return None;
            }
            self.event_zero_anchor_sample =
                Some(packet_start_sample - event_number * interval_samples);
            event_number
        };

        let event_counter = self.event_counter_for_index(event_index);
        let expected_channel = self.channel_selector.channel_for_event(event_counter);
        if expected_channel != packet.channel_index {
            self.timing_misses.channel += 1;
            if self.last_event_index.is_none() {
                self.event_zero_anchor_sample = None;
            }
            return None;
        }

        if self
            .last_event_index
            .is_none_or(|last_event| event_index >= last_event)
        {
            self.event_zero_anchor_sample = Some(
                packet_start_sample.saturating_sub(event_index.saturating_mul(interval_samples)),
            );
            self.last_event_index = Some(event_index);
        }
        Some(ConnectionEventObservation {
            event_counter,
            channel_index: expected_channel,
            counter_exact: self.event_counter_exact,
            event_index,
        })
    }

    fn recover_data_timing(
        &mut self,
        channel_index: u8,
        packet_start_sample: u64,
        interval_samples: u64,
    ) -> Option<ConnectionEventObservation> {
        let channel_selector = self.channel_selector.clone();
        let recovery = self
            .timing_recovery
            .get_or_insert_with(|| TimingRecovery::new(self.recovery_counter_hint));
        let resolution = recovery.observe(
            &channel_selector,
            interval_samples,
            packet_start_sample,
            channel_index,
        )?;

        let event_index = packet_start_sample / interval_samples;
        self.event_zero_anchor_sample = Some(packet_start_sample % interval_samples);
        self.last_event_index = Some(event_index);
        self.event_counter_offset = resolution.event_counter.wrapping_sub(event_index as u16);
        self.event_counter_exact = resolution.counter_exact;
        self.timing_discontinuous = false;
        self.timing_recovery = None;
        self.recovery_counter_hint = None;
        self.relock_count += 1;

        Some(ConnectionEventObservation {
            event_counter: resolution.event_counter,
            channel_index,
            counter_exact: resolution.counter_exact,
            event_index,
        })
    }

    fn observe_control_pdu(
        &mut self,
        packet: &BleDataPacket,
        observation: Option<ConnectionEventObservation>,
        sample_rate_hz: u64,
    ) -> ControlObservation {
        let mut result = ControlObservation::default();
        if self.encrypted {
            return result;
        }
        let control = match parse_control_pdu(packet) {
            Ok(Some(control)) => control,
            Ok(None) => return result,
            Err(_) => {
                result.parse_errors = 1;
                return result;
            }
        };
        result.parsed = 1;

        match control {
            BleControlPdu::ConnectionUpdateInd(parameters) => {
                let Some(instant_sample) = observation.and_then(|observation| {
                    self.sample_for_instant(observation, parameters.instant, sample_rate_hz)
                }) else {
                    result.unresolved_updates = 1;
                    return result;
                };
                if !valid_connection_update(parameters) {
                    result.parse_errors = 1;
                    return result;
                }
                self.pending_connection_update = Some(PendingConnectionUpdate {
                    parameters,
                    instant_sample,
                });
                result.connection_updates_scheduled = 1;
            }
            BleControlPdu::ChannelMapInd(parameters) => {
                let Some(instant_sample) = observation.and_then(|observation| {
                    self.sample_for_instant(observation, parameters.instant, sample_rate_hz)
                }) else {
                    result.unresolved_updates = 1;
                    return result;
                };
                if BleChannelSelector::new(
                    self.channel_selector.algorithm(),
                    self.access_address,
                    self.hop_increment,
                    parameters.channel_map,
                )
                .is_err()
                {
                    result.parse_errors = 1;
                    return result;
                }
                self.pending_channel_map_update = Some(PendingChannelMapUpdate {
                    parameters,
                    instant_sample,
                });
                result.channel_map_updates_scheduled = 1;
            }
            BleControlPdu::TerminateInd { .. } => result.terminate = true,
            BleControlPdu::StartEncryptionReq => {}
            BleControlPdu::StartEncryptionRsp => {
                self.encrypted = true;
                result.encryption_starts = 1;
            }
            BleControlPdu::Unknown { .. } => {}
        }
        result
    }

    fn sample_for_instant(
        &self,
        observation: ConnectionEventObservation,
        instant: u16,
        sample_rate_hz: u64,
    ) -> Option<u64> {
        if !observation.counter_exact {
            return None;
        }
        let event_delta = instant.wrapping_sub(observation.event_counter);
        if event_delta >= 32_767 {
            return None;
        }
        let interval_samples = units_1_25_ms_to_samples(self.interval_units as u64, sample_rate_hz);
        let anchor = self.event_zero_anchor_sample?;
        anchor.checked_add(
            observation
                .event_index
                .checked_add(event_delta as u64)?
                .checked_mul(interval_samples)?,
        )
    }

    fn apply_pending_updates(&mut self, packet_start_sample: u64) {
        if let Some(update) = self
            .pending_channel_map_update
            .take_if(|update| packet_start_sample >= update.instant_sample)
            && let Ok(selector) = BleChannelSelector::new(
                self.channel_selector.algorithm(),
                self.access_address,
                self.hop_increment,
                update.parameters.channel_map,
            )
        {
            self.channel_selector = selector;
            self.channel_map_updates_applied += 1;
        }

        if let Some(update) = self
            .pending_connection_update
            .take_if(|update| packet_start_sample >= update.instant_sample)
        {
            self.interval_units = update.parameters.interval_units;
            self.window_size_units = update.parameters.window_size_units;
            self.window_offset_units = update.parameters.window_offset_units;
            self.latency = update.parameters.latency;
            self.timeout_units = update.parameters.timeout_units;
            self.connect_end_sample = Some(update.instant_sample);
            self.event_zero_anchor_sample = None;
            self.last_event_index = None;
            self.event_counter_offset = update.parameters.instant;
            self.event_counter_exact = true;
            self.timing_discontinuous = false;
            self.timing_recovery = None;
            self.recovery_counter_hint = None;
            self.connection_updates_applied += 1;
        }
    }

    fn event_counter_for_index(&self, event_index: u64) -> u16 {
        (event_index as u16).wrapping_add(self.event_counter_offset)
    }

    fn timing_summary(&self) -> String {
        let misses = self.timing_misses.total();
        if self.timing_discontinuous {
            let candidates = self
                .timing_recovery
                .as_ref()
                .map_or(0, TimingRecovery::candidate_count);
            return format!("discontinuous(candidates={candidates},misses={misses})");
        }
        match self.last_event_index {
            Some(event) => {
                let event_counter = self.event_counter_for_index(event);
                let lock = if self.event_counter_exact {
                    "locked"
                } else {
                    "phase_locked"
                };
                format!(
                    "{lock}(last_event={event_counter},relocks={},misses={misses})",
                    self.relock_count
                )
            }
            None if self.connect_end_sample.is_some() => {
                format!("searching(misses={misses})")
            }
            None => "unavailable".to_string(),
        }
    }

    fn monitored_event_hits(&self) -> String {
        let start = self.last_event_index.map_or(0, |event| {
            self.event_counter_for_index(event).wrapping_add(1)
        });
        self.channel_selector
            .monitored_event_hits(&self.monitored_data_channels, start, 128, 8)
    }

    fn follow_plan(&self, count: usize) -> Option<ConnectionFollowPlan> {
        if self.pending_connection_update.is_some() || self.pending_channel_map_update.is_some() {
            return None;
        }
        let sample_rate_hz = self.sample_rate_hz?;
        let last_event_index = self.last_event_index?;
        let event_zero_sample = self.event_zero_anchor_sample?;
        let interval_samples = units_1_25_ms_to_samples(self.interval_units as u64, sample_rate_hz);
        let planner = ConnectionEventPlanner::with_event_counter_offset(
            self.channel_selector.clone(),
            event_zero_sample,
            interval_samples,
            self.event_counter_offset,
        )?;
        Some(ConnectionFollowPlan {
            access_address: self.access_address,
            events: planner.plan_after(last_event_index, count),
        })
    }

    fn event_overlaps(
        &self,
        channel_index: u8,
        start_sample: u64,
        end_sample: u64,
        guard_samples: u64,
    ) -> Option<bool> {
        let pending_instant = self
            .pending_connection_update
            .map(|update| update.instant_sample)
            .into_iter()
            .chain(
                self.pending_channel_map_update
                    .map(|update| update.instant_sample),
            )
            .min();
        if pending_instant.is_some_and(|instant| end_sample >= instant) {
            return None;
        }
        let sample_rate_hz = self.sample_rate_hz?;
        let event_zero_sample = self.event_zero_anchor_sample?;
        self.last_event_index?;
        let interval_samples = units_1_25_ms_to_samples(self.interval_units as u64, sample_rate_hz);
        let planner = ConnectionEventPlanner::with_event_counter_offset(
            self.channel_selector.clone(),
            event_zero_sample,
            interval_samples,
            self.event_counter_offset,
        )?;
        Some(
            planner
                .event_overlapping(channel_index, start_sample, end_sample, guard_samples)
                .is_some(),
        )
    }

    fn invalidate_timing(&mut self) {
        if let Some(last_event_index) = self.last_event_index {
            self.recovery_counter_hint = Some(self.event_counter_for_index(last_event_index));
        }
        self.connect_end_sample = None;
        self.event_zero_anchor_sample = None;
        self.last_event_index = None;
        self.timing_recovery = None;
        self.pending_connection_update = None;
        self.pending_channel_map_update = None;
        self.timing_discontinuous = true;
    }
}

fn valid_connection_update(update: ConnectionUpdateInd) -> bool {
    (1..=8).contains(&update.window_size_units)
        && (6..=3200).contains(&update.interval_units)
        && (update.window_size_units as u16) < update.interval_units
        && update.window_offset_units < update.interval_units
        && update.latency <= 499
        && (10..=3200).contains(&update.timeout_units)
        && (update.timeout_units as u32) * 4
            > (update.latency as u32 + 1) * update.interval_units as u32
}

fn units_1_25_ms_to_samples(units: u64, sample_rate_hz: u64) -> u64 {
    sample_rate_hz.saturating_mul(units) / 800
}

#[derive(Default)]
pub(crate) struct ConnectionTable {
    connections: Vec<BleConnection>,
    monitored_data_channels: Vec<u8>,
    last_overflow_sequence: Option<u64>,
    overflow_events: u64,
    tracked_data_packets: u64,
    connect_ind_seen: u64,
    relocks: u64,
    recovery_observations: u64,
    recovery_resets: u64,
    control_pdus: u64,
    control_parse_errors: u64,
    unresolved_control_updates: u64,
    connection_updates_scheduled: u64,
    connection_updates_applied: u64,
    channel_map_updates_scheduled: u64,
    channel_map_updates_applied: u64,
    terminated_connections: u64,
    encryption_starts: u64,
    expired_connections: u64,
}

impl ConnectionTable {
    fn with_monitored_channels(channels: &[u8]) -> Self {
        let mut monitored_data_channels = channels
            .iter()
            .copied()
            .filter(|channel| *channel <= 36)
            .collect::<Vec<_>>();
        monitored_data_channels.sort_unstable();
        monitored_data_channels.dedup();
        Self {
            connections: Vec::new(),
            monitored_data_channels,
            last_overflow_sequence: None,
            overflow_events: 0,
            tracked_data_packets: 0,
            connect_ind_seen: 0,
            relocks: 0,
            recovery_observations: 0,
            recovery_resets: 0,
            control_pdus: 0,
            control_parse_errors: 0,
            unresolved_control_updates: 0,
            connection_updates_scheduled: 0,
            connection_updates_applied: 0,
            channel_map_updates_scheduled: 0,
            channel_map_updates_applied: 0,
            terminated_connections: 0,
            encryption_starts: 0,
            expired_connections: 0,
        }
    }

    pub(crate) fn observe_packet(&mut self, packet: &BlePacket) {
        self.observe_packet_with_timing(packet, None);
    }

    pub(crate) fn observe_packet_at(
        &mut self,
        packet: &BlePacket,
        sample_index: u64,
        sample_rate_hz: u64,
    ) {
        self.observe_packet_with_timing(packet, Some((sample_index, sample_rate_hz)));
    }

    fn observe_packet_with_timing(&mut self, packet: &BlePacket, timing: Option<(u64, u64)>) {
        if let Some((sample_index, sample_rate_hz)) = timing {
            self.expire_stale_connections(sample_index, sample_rate_hz);
        }
        let Some(mut connection) =
            BleConnection::from_connect_ind(packet, &self.monitored_data_channels, timing)
        else {
            return;
        };
        self.connect_ind_seen += 1;

        if let Some(existing) = self
            .connections
            .iter_mut()
            .find(|entry| entry.access_address == connection.access_address)
        {
            connection.seen_count = existing.seen_count + 1;
            connection.data_packet_count = existing.data_packet_count;
            *existing = connection;
            return;
        }

        if let Some(existing) = self.connections.iter_mut().find(|entry| {
            entry.initiator_address == connection.initiator_address
                && entry.advertiser_address == connection.advertiser_address
        }) {
            connection.seen_count = existing.seen_count + 1;
            *existing = connection;
            return;
        }

        if self.connections.len() >= MAX_TRACKED_CONNECTIONS {
            let oldest = self
                .connections
                .iter()
                .enumerate()
                .min_by_key(|(_, connection)| connection.last_activity)
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.connections.swap_remove(oldest);
        }
        self.connections.push(connection);
    }

    pub(crate) fn observe_data_packet_at(
        &mut self,
        packet: &BleDataPacket,
        sample_index: u64,
        sample_rate_hz: u64,
    ) -> Option<ConnectionEventObservation> {
        self.observe_data_packet_with_timing(packet, Some((sample_index, sample_rate_hz)))
    }

    fn observe_data_packet_with_timing(
        &mut self,
        packet: &BleDataPacket,
        timing: Option<(u64, u64)>,
    ) -> Option<ConnectionEventObservation> {
        let connection_index = self
            .connections
            .iter()
            .position(|entry| entry.access_address == packet.access_address)?;
        let (observation, recovery_observation, relocked, control, applied_connection, applied_map) = {
            let connection = &mut self.connections[connection_index];
            let recovery_observation = timing.is_some() && connection.timing_discontinuous;
            let old_connection_updates = connection.connection_updates_applied;
            let old_channel_map_updates = connection.channel_map_updates_applied;
            connection.last_channel = packet.channel_index;
            connection.data_packet_count += 1;
            if let Some((sample_index, _)) = timing {
                connection.last_activity_sample = Some(sample_index);
            }
            connection.last_activity = Instant::now();
            let observation = timing.and_then(|(sample_index, sample_rate_hz)| {
                connection.observe_data_timing(packet, sample_index, sample_rate_hz)
            });
            let control = connection.observe_control_pdu(
                packet,
                observation,
                timing.map_or(0, |(_, sample_rate_hz)| sample_rate_hz),
            );
            let relocked = recovery_observation && !connection.timing_discontinuous;
            (
                observation,
                recovery_observation,
                relocked,
                control,
                connection
                    .connection_updates_applied
                    .saturating_sub(old_connection_updates),
                connection
                    .channel_map_updates_applied
                    .saturating_sub(old_channel_map_updates),
            )
        };
        self.tracked_data_packets += 1;
        self.recovery_observations += u64::from(recovery_observation);
        self.relocks += u64::from(relocked);
        self.control_pdus += control.parsed;
        self.control_parse_errors += control.parse_errors;
        self.unresolved_control_updates += control.unresolved_updates;
        self.connection_updates_scheduled += control.connection_updates_scheduled;
        self.connection_updates_applied += applied_connection;
        self.channel_map_updates_scheduled += control.channel_map_updates_scheduled;
        self.channel_map_updates_applied += applied_map;
        self.encryption_starts += control.encryption_starts;
        if control.terminate {
            self.connections.swap_remove(connection_index);
            self.terminated_connections += 1;
        }
        observation
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &BleConnection> {
        self.connections.iter()
    }

    fn decode_contexts(&self) -> Vec<ConnectionDecodeContext> {
        self.connections
            .iter()
            .map(|connection| ConnectionDecodeContext {
                access_address: connection.access_address,
                crc_init: connection.crc_init,
                phy: connection.phy,
            })
            .collect()
    }

    fn decode_contexts_for_window(
        &self,
        channel_index: u8,
        start_sample: u64,
        end_sample: u64,
        guard_samples: u64,
    ) -> ConnectionDecodeSelection {
        self.connections.iter().fold(
            ConnectionDecodeSelection::default(),
            |mut selection, connection| {
                let include = match connection.event_overlaps(
                    channel_index,
                    start_sample,
                    end_sample,
                    guard_samples,
                ) {
                    Some(overlaps) => {
                        selection.locked_checks += 1;
                        selection.window_matches += u64::from(overlaps);
                        overlaps
                    }
                    None => {
                        selection.searching_checks += 1;
                        true
                    }
                };
                if include {
                    selection.contexts.push(ConnectionDecodeContext {
                        access_address: connection.access_address,
                        crc_init: connection.crc_init,
                        phy: connection.phy,
                    });
                }
                selection
            },
        )
    }

    fn observe_overflow(&mut self, sequence: u64) {
        if self
            .last_overflow_sequence
            .is_some_and(|last| sequence <= last)
        {
            return;
        }
        self.last_overflow_sequence = Some(sequence);
        self.overflow_events = sequence;
        self.unresolved_control_updates += self
            .connections
            .iter()
            .map(|connection| {
                u64::from(connection.pending_connection_update.is_some())
                    + u64::from(connection.pending_channel_map_update.is_some())
            })
            .sum::<u64>();
        self.recovery_resets += self
            .connections
            .iter()
            .filter(|connection| connection.timing_recovery.is_some())
            .count() as u64;
        for connection in &mut self.connections {
            connection.invalidate_timing();
        }
    }

    fn expire_stale_connections(&mut self, sample_index: u64, sample_rate_hz: u64) {
        let mut expired = 0;
        let mut unresolved_updates = 0;
        self.connections.retain(|connection| {
            let stale = connection.is_stale_at(sample_index, sample_rate_hz);
            if stale {
                expired += 1;
                unresolved_updates += u64::from(connection.pending_connection_update.is_some())
                    + u64::from(connection.pending_channel_map_update.is_some());
            }
            !stale
        });
        self.expired_connections += expired;
        self.unresolved_control_updates += unresolved_updates;
    }
}

#[derive(Clone)]
pub(crate) struct SharedConnectionTable {
    inner: Arc<RwLock<ConnectionTable>>,
    reporters_remaining: Arc<AtomicUsize>,
}

impl Default for SharedConnectionTable {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ConnectionTable::default())),
            reporters_remaining: Arc::new(AtomicUsize::new(1)),
        }
    }
}

impl SharedConnectionTable {
    pub(crate) fn with_monitored_channels(channels: &[u8]) -> Self {
        let mut reporter_channels = channels.to_vec();
        reporter_channels.sort_unstable();
        reporter_channels.dedup();
        Self {
            inner: Arc::new(RwLock::new(ConnectionTable::with_monitored_channels(
                channels,
            ))),
            reporters_remaining: Arc::new(AtomicUsize::new(reporter_channels.len().max(1))),
        }
    }

    pub(crate) fn observe_packet(&self, packet: &BlePacket) {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe_packet(packet);
    }

    pub(crate) fn observe_packet_at(
        &self,
        packet: &BlePacket,
        sample_index: u64,
        sample_rate_hz: u64,
    ) {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe_packet_at(packet, sample_index, sample_rate_hz);
    }

    pub(crate) fn observe_data_packet_at(
        &self,
        packet: &BleDataPacket,
        sample_index: u64,
        sample_rate_hz: u64,
    ) -> Option<ConnectionEventObservation> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe_data_packet_at(packet, sample_index, sample_rate_hz)
    }

    pub(crate) fn snapshot(&self) -> Vec<BleConnection> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn decode_contexts(&self) -> Vec<ConnectionDecodeContext> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_contexts()
    }

    pub(crate) fn decode_contexts_for_window(
        &self,
        channel_index: u8,
        start_sample: u64,
        end_sample: u64,
        guard_samples: u64,
    ) -> ConnectionDecodeSelection {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_contexts_for_window(channel_index, start_sample, end_sample, guard_samples)
    }

    pub(crate) fn observe_overflow(&self, sequence: u64) {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observe_overflow(sequence);
    }

    pub(crate) fn follow_plans(&self, count: usize) -> Vec<ConnectionFollowPlan> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter_map(|connection| connection.follow_plan(count))
            .collect()
    }

    pub(crate) fn finish_reporter(&self) -> bool {
        self.reporters_remaining.fetch_sub(1, Ordering::AcqRel) == 1
    }

    pub(crate) fn tracking_summary(&self) -> ConnectionTrackingSummary {
        let table = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut summary = table.iter().fold(
            ConnectionTrackingSummary::default(),
            |mut summary, connection| {
                summary.connections += 1;
                summary.timing_sample_rate_misses += connection.timing_misses.sample_rate;
                summary.timing_invalid_parameter_misses +=
                    connection.timing_misses.invalid_parameters;
                summary.timing_before_window_misses += connection.timing_misses.before_window;
                summary.timing_outside_window_misses += connection.timing_misses.outside_window;
                summary.timing_channel_misses += connection.timing_misses.channel;
                summary.encrypted_connections += u64::from(connection.encrypted);
                summary.pending_connection_updates +=
                    u64::from(connection.pending_connection_update.is_some());
                summary.pending_channel_map_updates +=
                    u64::from(connection.pending_channel_map_update.is_some());
                if connection.timing_discontinuous {
                    summary.discontinuous_connections += 1;
                    summary.recovery_candidates += connection
                        .timing_recovery
                        .as_ref()
                        .map_or(0, |recovery| recovery.candidate_count() as u64);
                } else if connection.last_event_index.is_some() {
                    summary.locked_connections += 1;
                } else {
                    summary.searching_connections += 1;
                }
                summary
            },
        );
        summary.connect_ind_seen = table.connect_ind_seen;
        summary.data_packets = table.tracked_data_packets;
        summary.overflow_events = table.overflow_events;
        summary.relocks = table.relocks;
        summary.recovery_observations = table.recovery_observations;
        summary.recovery_resets = table.recovery_resets;
        summary.expired_connections = table.expired_connections;
        summary.control_pdus = table.control_pdus;
        summary.control_parse_errors = table.control_parse_errors;
        summary.unresolved_control_updates = table.unresolved_control_updates;
        summary.connection_updates_scheduled = table.connection_updates_scheduled;
        summary.connection_updates_applied = table.connection_updates_applied;
        summary.channel_map_updates_scheduled = table.channel_map_updates_scheduled;
        summary.channel_map_updates_applied = table.channel_map_updates_applied;
        summary.terminated_connections = table.terminated_connections;
        summary.encryption_starts = table.encryption_starts;
        summary
    }
}

pub(crate) fn format_connection_summary(connection: &BleConnection) -> String {
    format!(
        "aa=0x{:08X} crc_init=0x{:06X} phy={} init_a={}{} adv_a={}{} win={:.2}ms offset={:.2}ms interval={:.2}ms latency={} timeout={}ms retention={}ms csa={} hop={} sca={} used={:?} events=[{}] monitored={:?} monitored_events=[{}] timing={} encrypted={} pending_updates={}/{} first_ch={} last_ch={} connect_ind_seen={} data_packets={}",
        connection.access_address,
        connection.crc_init,
        connection.phy,
        ble_protocol::format_addr(&connection.initiator_address),
        addr_type_suffix(connection.initiator_random),
        ble_protocol::format_addr(&connection.advertiser_address),
        addr_type_suffix(connection.advertiser_random),
        connection.window_size_ms(),
        connection.window_offset_ms(),
        connection.interval_ms(),
        connection.latency,
        connection.timeout_ms(),
        connection.retention_ms(),
        connection.channel_selector.algorithm(),
        connection.hop_increment,
        connection.sca,
        connection.channel_selector.used_channels(),
        connection.channel_selector.event_preview(8),
        connection.monitored_data_channels,
        connection.monitored_event_hits(),
        connection.timing_summary(),
        connection.encrypted,
        u8::from(connection.pending_connection_update.is_some()),
        u8::from(connection.pending_channel_map_update.is_some()),
        connection.first_channel,
        connection.last_channel,
        connection.seen_count,
        connection.data_packet_count,
    )
}

fn addr_type_suffix(random: bool) -> &'static str {
    if random { "(random)" } else { "(public)" }
}

#[cfg(test)]
#[path = "ble_connection_tests.rs"]
mod tests;
