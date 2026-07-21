use std::error::Error;
use std::fmt;

use futuredsp::Filter;
use futuredsp::FirFilter;
use futuresdr::num_complex::Complex32;

use crate::ble_burst_catcher::CapturedBurst;
use crate::ble_burst_catcher::PRE_TRIGGER_SYMBOLS;
use crate::ble_connection::ConnectionDecodeContext;
use crate::ble_data::BLE_MAX_DATA_PAYLOAD_LEN;
use crate::ble_data::BleDataPacket;
use crate::ble_data::parse_data_pdu;
use crate::ble_detector::DetectorEvent;
use crate::ble_detector::DetectorStats;
use crate::ble_detector::PacketDetector;
use crate::ble_protocol::BleAdvPduType;
use crate::ble_protocol::BlePacket;
use crate::ble_protocol::BlePhy;
use crate::diagnostics as debug_output;

const MIN_BURST_SAMPLES: usize = 96;
const PHASE_DUPLICATE_WINDOW_SYMBOLS: usize = 4;
const CFO_MEDIAN_SYMBOLS: usize = 64;
const MAX_FREQ_OFFSET: f32 = 0.85;

type GmskFir = FirFilter<Complex32, Complex32, Vec<f32>>;

pub(crate) struct BurstDecoder {
    phy: BlePhy,
    samples_per_symbol: usize,
    channel_index: u8,
    filter: GmskFir,
}

impl BurstDecoder {
    pub(crate) fn new(
        phy: BlePhy,
        samples_per_symbol: usize,
        channel_index: u8,
        filter_taps: Vec<f32>,
    ) -> Result<Self, UnsupportedPhy> {
        if phy != BlePhy::Le1M {
            return Err(UnsupportedPhy { phy });
        }

        Ok(Self {
            phy,
            samples_per_symbol,
            channel_index,
            filter: FirFilter::new(filter_taps),
        })
    }

    pub(crate) fn decode(
        &self,
        burst: &CapturedBurst,
        connections: &[ConnectionDecodeContext],
        decode_advertising: bool,
        diagnostics: bool,
    ) -> BurstDecodeOutcome {
        if burst.samples.len() < MIN_BURST_SAMPLES {
            return BurstDecodeOutcome::Short;
        }

        let Some(filtered) = filter_burst(&burst.samples, &self.filter) else {
            return BurstDecodeOutcome::Short;
        };
        let cfo_start = PRE_TRIGGER_SYMBOLS * self.samples_per_symbol
            + self.filter.length().saturating_sub(1) / 2;
        let Some(demod) = demodulate_burst(&filtered, self.samples_per_symbol, cfo_start) else {
            return BurstDecodeOutcome::FskRejected;
        };

        let data = decode_data_candidates(
            &demod,
            self.samples_per_symbol,
            self.channel_index,
            connections,
        );

        if !decode_advertising {
            return BurstDecodeOutcome::Decoded(BurstDecodeResult {
                candidates: Vec::new(),
                detector_stats: DetectorStats::default(),
                phase_valid_packets: vec![0; self.samples_per_symbol],
                phase_only_packets: vec![0; self.samples_per_symbol],
                phase_duplicates: 0,
                data,
            });
        }

        let mut detector_stats = DetectorStats::default();
        let mut candidates = Vec::new();

        for phase in 0..self.samples_per_symbol {
            let mut detector = PacketDetector::with_packet_output(phase, self.channel_index, false);

            for (symbol_index, value) in demod
                .iter()
                .skip(phase)
                .step_by(self.samples_per_symbol)
                .enumerate()
            {
                match detector.process_symbol(u8::from(*value > 0.0)) {
                    DetectorEvent::None | DetectorEvent::HeaderRejected => {}
                    DetectorEvent::ValidPacket { packet } => {
                        candidates.push(BurstPacketCandidate {
                            sample_index: phase + symbol_index * self.samples_per_symbol,
                            phase,
                            packet,
                        });
                    }
                    DetectorEvent::CrcRejected { reason } => {
                        detector.add_crc_reject();
                        if diagnostics && debug_output::enabled() {
                            println!("BLE burst candidate rejected: phase={phase} {reason}");
                        }
                    }
                }
            }

            let mut phase_stats = detector.stats();
            phase_stats.packets = 0;
            phase_stats.connect_ind_packets = 0;
            detector_stats.merge(phase_stats);
        }

        let candidate_count = candidates.len();
        let (phase_valid_packets, phase_only_packets) =
            phase_candidate_counts(&candidates, self.samples_per_symbol);
        let candidates = deduplicate_phase_candidates(candidates, self.samples_per_symbol);
        let phase_duplicates = (candidate_count - candidates.len()) as u64;

        detector_stats.packets = candidates.len() as u64;
        detector_stats.connect_ind_packets = candidates
            .iter()
            .filter(|candidate| candidate.packet.pdu_type == BleAdvPduType::ConnectInd)
            .count() as u64;

        debug_assert!(
            candidates
                .iter()
                .all(|candidate| candidate.packet.phy == self.phy)
        );

        BurstDecodeOutcome::Decoded(BurstDecodeResult {
            candidates,
            detector_stats,
            phase_valid_packets,
            phase_only_packets,
            phase_duplicates,
            data,
        })
    }

    pub(crate) fn samples_per_symbol(&self) -> usize {
        self.samples_per_symbol
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedPhy {
    pub phy: BlePhy,
}

impl fmt::Display for UnsupportedPhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{0} burst decoding is not implemented", self.phy)
    }
}

impl Error for UnsupportedPhy {}

pub(crate) enum BurstDecodeOutcome {
    Short,
    FskRejected,
    Decoded(BurstDecodeResult),
}

pub(crate) struct BurstDecodeResult {
    pub(crate) candidates: Vec<BurstPacketCandidate>,
    pub(crate) detector_stats: DetectorStats,
    pub(crate) phase_valid_packets: Vec<u64>,
    pub(crate) phase_only_packets: Vec<u64>,
    pub(crate) phase_duplicates: u64,
    pub(crate) data: DataDecodeResult,
}

#[derive(Default)]
pub(crate) struct DataDecodeResult {
    pub(crate) candidates: Vec<BurstDataPacketCandidate>,
    pub(crate) aa_candidates: u64,
    pub(crate) crc_rejects: u64,
    pub(crate) phase_duplicates: u64,
}

pub(crate) struct BurstPacketCandidate {
    pub(crate) sample_index: usize,
    pub(crate) phase: usize,
    pub(crate) packet: BlePacket,
}

pub(crate) struct BurstDataPacketCandidate {
    pub(crate) sample_index: usize,
    pub(crate) phase: usize,
    pub(crate) packet: BleDataPacket,
}

fn deduplicate_phase_candidates(
    mut candidates: Vec<BurstPacketCandidate>,
    samples_per_symbol: usize,
) -> Vec<BurstPacketCandidate> {
    candidates.sort_unstable_by_key(|candidate| candidate.sample_index);
    let duplicate_window = PHASE_DUPLICATE_WINDOW_SYMBOLS * samples_per_symbol;
    let mut unique: Vec<BurstPacketCandidate> = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let duplicate = unique
            .iter()
            .rev()
            .take_while(|previous| {
                candidate.sample_index.saturating_sub(previous.sample_index) <= duplicate_window
            })
            .any(|previous| candidate.packet == previous.packet);
        if !duplicate {
            unique.push(candidate);
        }
    }

    unique
}

fn phase_candidate_counts(
    candidates: &[BurstPacketCandidate],
    samples_per_symbol: usize,
) -> (Vec<u64>, Vec<u64>) {
    let duplicate_window = PHASE_DUPLICATE_WINDOW_SYMBOLS * samples_per_symbol;
    let mut phase_valid_packets = vec![0; samples_per_symbol];
    let mut phase_only_packets = vec![0; samples_per_symbol];

    for (index, candidate) in candidates.iter().enumerate() {
        phase_valid_packets[candidate.phase] += 1;
        let shared_with_another_phase =
            candidates.iter().enumerate().any(|(other_index, other)| {
                index != other_index
                    && candidate.phase != other.phase
                    && candidate.sample_index.abs_diff(other.sample_index) <= duplicate_window
                    && candidate.packet == other.packet
            });
        if !shared_with_another_phase {
            phase_only_packets[candidate.phase] += 1;
        }
    }

    (phase_valid_packets, phase_only_packets)
}

fn decode_data_candidates(
    demod: &[f32],
    samples_per_symbol: usize,
    channel_index: u8,
    connections: &[ConnectionDecodeContext],
) -> DataDecodeResult {
    if channel_index >= 37 || connections.is_empty() {
        return DataDecodeResult::default();
    }

    let mut result = DataDecodeResult::default();
    let mut candidates = Vec::new();

    for phase in 0..samples_per_symbol {
        let bits: Vec<_> = demod
            .iter()
            .skip(phase)
            .step_by(samples_per_symbol)
            .map(|value| u8::from(*value > 0.0))
            .collect();

        for connection in connections
            .iter()
            .filter(|connection| connection.phy == BlePhy::Le1M)
        {
            scan_data_phase(
                &bits,
                phase,
                samples_per_symbol,
                channel_index,
                *connection,
                &mut result,
                &mut candidates,
            );
        }
    }

    let candidate_count = candidates.len();
    result.candidates = deduplicate_data_candidates(candidates, samples_per_symbol);
    result.phase_duplicates = (candidate_count - result.candidates.len()) as u64;
    result
}

fn scan_data_phase(
    bits: &[u8],
    phase: usize,
    samples_per_symbol: usize,
    channel_index: u8,
    connection: ConnectionDecodeContext,
    result: &mut DataDecodeResult,
    candidates: &mut Vec<BurstDataPacketCandidate>,
) {
    let mut shift_reg = 0u32;
    let mut bit_index = 0usize;

    while bit_index < bits.len() {
        shift_reg = (shift_reg >> 1) | ((bits[bit_index] as u32) << 31);
        let inverted = if shift_reg == connection.access_address {
            Some(false)
        } else if shift_reg == !connection.access_address {
            Some(true)
        } else {
            None
        };

        let Some(inverted) = inverted else {
            bit_index += 1;
            continue;
        };

        result.aa_candidates += 1;
        let pdu_start = bit_index + 1;
        match decode_data_candidate(&bits[pdu_start..], inverted, channel_index, connection) {
            DataCandidateOutcome::Incomplete => break,
            DataCandidateOutcome::Rejected => {
                result.crc_rejects += 1;
                bit_index += 1;
            }
            DataCandidateOutcome::Valid {
                packet,
                consumed_bits,
            } => {
                candidates.push(BurstDataPacketCandidate {
                    sample_index: phase + bit_index * samples_per_symbol,
                    phase,
                    packet,
                });
                bit_index = pdu_start + consumed_bits;
                shift_reg = 0;
            }
        }
    }
}

enum DataCandidateOutcome {
    Incomplete,
    Rejected,
    Valid {
        packet: BleDataPacket,
        consumed_bits: usize,
    },
}

fn decode_data_candidate(
    bits: &[u8],
    inverted: bool,
    channel_index: u8,
    connection: ConnectionDecodeContext,
) -> DataCandidateOutcome {
    if bits.len() < 16 {
        return DataCandidateOutcome::Incomplete;
    }

    let header = bits_to_bytes(&bits[..16], inverted);
    let Ok(header) = crate::ble_phy::apply_whitening(channel_index, &header) else {
        return DataCandidateOutcome::Rejected;
    };
    let payload_len = header[1] as usize;
    if payload_len > BLE_MAX_DATA_PAYLOAD_LEN {
        return DataCandidateOutcome::Rejected;
    }

    let needed_bits = (2 + payload_len + 3) * 8;
    if bits.len() < needed_bits {
        return DataCandidateOutcome::Incomplete;
    }

    let whitened_pdu_crc = bits_to_bytes(&bits[..needed_bits], inverted);
    match parse_data_pdu(
        channel_index,
        connection.phy,
        connection.access_address,
        connection.crc_init,
        &whitened_pdu_crc,
    ) {
        Ok(packet) => DataCandidateOutcome::Valid {
            packet,
            consumed_bits: needed_bits,
        },
        Err(_) => DataCandidateOutcome::Rejected,
    }
}

fn bits_to_bytes(bits: &[u8], inverted: bool) -> Vec<u8> {
    bits.chunks_exact(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0u8, |byte, (bit_index, bit)| {
                    byte | (((*bit ^ u8::from(inverted)) & 1) << bit_index)
                })
        })
        .collect()
}

fn deduplicate_data_candidates(
    mut candidates: Vec<BurstDataPacketCandidate>,
    samples_per_symbol: usize,
) -> Vec<BurstDataPacketCandidate> {
    candidates.sort_unstable_by_key(|candidate| candidate.sample_index);
    let duplicate_window = PHASE_DUPLICATE_WINDOW_SYMBOLS * samples_per_symbol;
    let mut unique: Vec<BurstDataPacketCandidate> = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let duplicate = unique
            .iter()
            .rev()
            .take_while(|previous| {
                candidate.sample_index.saturating_sub(previous.sample_index) <= duplicate_window
            })
            .any(|previous| candidate.packet == previous.packet);
        if !duplicate {
            unique.push(candidate);
        }
    }

    unique
}

fn filter_burst(samples: &[Complex32], filter: &GmskFir) -> Option<Vec<Complex32>> {
    let output_len = samples.len().checked_add(1)?.checked_sub(filter.length())?;
    if output_len == 0 {
        return None;
    }

    let mut output = vec![Complex32::new(0.0, 0.0); output_len];
    let (_, produced, _) = filter.filter(samples, &mut output);
    output.truncate(produced);
    Some(output)
}

fn demodulate_burst(
    samples: &[Complex32],
    samples_per_symbol: usize,
    cfo_start: usize,
) -> Option<Vec<f32>> {
    if samples.len() < cfo_start + samples_per_symbol * CFO_MEDIAN_SYMBOLS {
        return None;
    }

    let mut demod = Vec::with_capacity(samples.len().saturating_sub(1));
    let mut prev = samples[0];
    for &sample in &samples[1..] {
        demod.push((sample * prev.conj()).arg() * std::f32::consts::FRAC_1_PI);
        prev = sample;
    }

    let (cfo, deviation) = estimate_cfo_and_deviation(&demod, samples_per_symbol, cfo_start)?;
    for value in &mut demod {
        *value = (*value - cfo) / deviation;
    }

    Some(demod)
}

fn estimate_cfo_and_deviation(
    demod: &[f32],
    samples_per_symbol: usize,
    start: usize,
) -> Option<(f32, f32)> {
    let end = start + samples_per_symbol * CFO_MEDIAN_SYMBOLS;
    if end > demod.len() {
        return None;
    }

    let mut pos = Vec::with_capacity(samples_per_symbol * CFO_MEDIAN_SYMBOLS);
    let mut neg = Vec::with_capacity(samples_per_symbol * CFO_MEDIAN_SYMBOLS);

    for &value in &demod[start..end] {
        if value.abs() > MAX_FREQ_OFFSET {
            return None;
        }
        if value > 0.0 {
            pos.push(value);
        } else if value < 0.0 {
            neg.push(value);
        }
    }

    if pos.len() < CFO_MEDIAN_SYMBOLS / 4 || neg.len() < CFO_MEDIAN_SYMBOLS / 4 {
        return None;
    }

    pos.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    neg.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let high = pos[pos.len() * 3 / 4];
    let low = neg[neg.len() / 4];
    let cfo = (high + low) * 0.5;
    let deviation = (high - cfo).abs();

    if deviation < 1e-6 {
        None
    } else {
        Some((cfo, deviation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_sim::generate_ble_packet_samples;
    use crate::gmsk_demod::gmsk_rx_taps;

    #[test]
    fn demodulates_simulated_burst() {
        let samples = generate_ble_packet_samples(2, 1, 37);
        let burst: Vec<_> = samples
            .into_iter()
            .filter(|sample| sample.norm() > 0.0)
            .collect();
        let demod = demodulate_burst(&burst, 2, PRE_TRIGGER_SYMBOLS * 2).unwrap();
        let mut detector = PacketDetector::new(0, 37);

        let mut decoded = false;
        for value in demod.iter().step_by(2) {
            if matches!(
                detector.process_symbol(u8::from(*value > 0.0)),
                DetectorEvent::ValidPacket { .. }
            ) {
                decoded = true;
                break;
            }
        }

        assert!(decoded);
    }

    #[test]
    fn decodes_multiple_packets_from_one_burst() {
        let packet: Vec<_> = generate_ble_packet_samples(2, 1, 37)
            .into_iter()
            .filter(|sample| sample.norm() > 0.0)
            .collect();
        let taps = gmsk_rx_taps(2).unwrap();
        let filter_history = taps.len().saturating_sub(1);
        let mut samples = vec![Complex32::new(0.0, 0.0); filter_history + PRE_TRIGGER_SYMBOLS * 2];
        samples.extend(packet.iter().copied());
        samples.extend(vec![Complex32::new(0.0, 0.0); 50]);
        samples.extend(packet);
        samples.extend(vec![Complex32::new(0.0, 0.0); 100]);

        let decoder = BurstDecoder::new(BlePhy::Le1M, 2, 37, taps).unwrap();
        let result = match decoder.decode(
            &CapturedBurst {
                samples,
                forced_close: false,
                start_sample_index: 0,
            },
            &[],
            true,
            false,
        ) {
            BurstDecodeOutcome::Decoded(result) => result,
            _ => panic!("burst should decode"),
        };

        assert_eq!(result.candidates.len(), 2);
        assert_eq!(result.detector_stats.packets, 2);
        assert_eq!(result.phase_valid_packets, [2, 2]);
        assert_eq!(result.phase_only_packets, [0, 0]);
        assert_eq!(result.phase_duplicates, 2);
    }

    #[test]
    fn decodes_connection_specific_data_packet_across_phases() {
        use crate::ble_data::build_data_pdu_crc;

        let connection = ConnectionDecodeContext {
            access_address: 0x1234_5678,
            crc_init: 0x00ab_cdef,
            phy: BlePhy::Le1M,
        };
        let payload = [0x03, 0x00, 0x04, 0x00];
        let whitened = build_data_pdu_crc(0, connection.crc_init, 0x02, &payload);
        let mut bits = Vec::new();
        for byte in connection.access_address.to_le_bytes() {
            bits.extend((0..8).map(|bit| (byte >> bit) & 1));
        }
        for byte in whitened {
            bits.extend((0..8).map(|bit| (byte >> bit) & 1));
        }
        let demod: Vec<_> = bits
            .into_iter()
            .flat_map(|bit| [bit, bit])
            .map(|bit| if bit == 0 { -1.0 } else { 1.0 })
            .collect();

        let result = decode_data_candidates(&demod, 2, 0, &[connection]);

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].packet.access_address, 0x1234_5678);
        assert_eq!(result.candidates[0].packet.payload, payload);
        assert_eq!(result.aa_candidates, 2);
        assert_eq!(result.phase_duplicates, 1);
        assert_eq!(result.crc_rejects, 0);
    }

    #[test]
    fn skips_advertising_scan_when_disabled() {
        let packet: Vec<_> = generate_ble_packet_samples(2, 1, 37)
            .into_iter()
            .filter(|sample| sample.norm() > 0.0)
            .collect();
        let taps = gmsk_rx_taps(2).unwrap();
        let filter_history = taps.len().saturating_sub(1);
        let mut samples = vec![Complex32::new(0.0, 0.0); filter_history + PRE_TRIGGER_SYMBOLS * 2];
        samples.extend(packet);

        let decoder = BurstDecoder::new(BlePhy::Le1M, 2, 0, taps).unwrap();
        let result = match decoder.decode(
            &CapturedBurst {
                samples,
                forced_close: false,
                start_sample_index: 0,
            },
            &[],
            false,
            false,
        ) {
            BurstDecodeOutcome::Decoded(result) => result,
            _ => panic!("burst should reach the decoder"),
        };

        assert!(result.candidates.is_empty());
        assert_eq!(result.detector_stats.aa_candidates, 0);
        assert_eq!(result.detector_stats.header_rejects, 0);
        assert_eq!(result.detector_stats.pdu_attempts(), 0);
        assert_eq!(result.phase_valid_packets, [0, 0]);
        assert_eq!(result.phase_only_packets, [0, 0]);
    }

    #[test]
    fn rejects_unsupported_phys_explicitly() {
        assert!(matches!(
            BurstDecoder::new(BlePhy::Le2M, 1, 37, vec![1.0]),
            Err(UnsupportedPhy { phy: BlePhy::Le2M })
        ));
        assert!(matches!(
            BurstDecoder::new(BlePhy::LeCoded, 2, 37, vec![1.0]),
            Err(UnsupportedPhy {
                phy: BlePhy::LeCoded
            })
        ));
    }
}
