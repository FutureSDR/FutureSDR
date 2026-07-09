use crate::ble_protocol;
use crate::ble_protocol::BlePacket;

const BLE_ACCESS_ADDRESS_INVERT: u32 = !ble_protocol::BLE_ACCESS_ADDRESS;
const MAX_HAMMING_DISTANCE: u32 = 2;
const BLE_CRC_LEN: usize = 3;
const PDU_OFFSET_RADIUS_BITS: usize = 2;
const PDU_OFFSET_COUNT: usize = PDU_OFFSET_RADIUS_BITS * 2 + 1;
const BLE_MAX_PDU_CRC_BITS: usize = (2 + ble_protocol::BLE_MAX_ADV_PAYLOAD_LEN + BLE_CRC_LEN) * 8;
const BLE_CAPTURE_WINDOW_BITS: usize = BLE_MAX_PDU_CRC_BITS + PDU_OFFSET_RADIUS_BITS * 2;

fn print_diagnostics() -> bool {
    cfg!(debug_assertions)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PacketState {
    SearchingAccessAddress,
    CapturingCandidateWindow,
}

#[derive(Clone, Debug)]
pub(crate) enum DetectorEvent {
    None,
    ValidPacket { packet: BlePacket },
    CrcRejected { reason: String },
    HeaderRejected,
}

pub(crate) struct PacketDetector {
    phase: usize,
    shift_reg: u32,
    state: PacketState,
    candidate_bits: Vec<u8>,
    channel_index: u8,
    invert_signal: bool,
    aa_candidates: u64,
    header_rejects: u64,
    packets: u64,
    crc_rejects: u64,
    duplicate_crc_rejects: u64,
    connect_ind_packets: u64,
    print_packets: bool,
}

impl PacketDetector {
    #[cfg(test)]
    pub(crate) fn new(phase: usize, channel_index: u8) -> Self {
        Self::with_packet_output(phase, channel_index, true)
    }

    pub(crate) fn with_packet_output(phase: usize, channel_index: u8, print_packets: bool) -> Self {
        Self {
            phase,
            shift_reg: 0,
            state: PacketState::SearchingAccessAddress,
            candidate_bits: Vec::new(),
            channel_index,
            invert_signal: false,
            aa_candidates: 0,
            header_rejects: 0,
            packets: 0,
            crc_rejects: 0,
            duplicate_crc_rejects: 0,
            connect_ind_packets: 0,
            print_packets,
        }
    }

    pub(crate) fn process_symbol(&mut self, mut bit: u8) -> DetectorEvent {
        if self.invert_signal {
            bit ^= 1;
        }

        self.shift_reg = (self.shift_reg >> 1) | ((bit as u32) << 31);

        match self.state {
            PacketState::SearchingAccessAddress => self.search_access_address(),
            PacketState::CapturingCandidateWindow => self.capture_candidate_bit(bit),
        }
    }

    pub(crate) fn reset_to_search(&mut self) {
        self.state = PacketState::SearchingAccessAddress;
        self.invert_signal = false;
        self.shift_reg = 0;
        self.candidate_bits.clear();
    }

    pub(crate) fn add_crc_reject(&mut self) {
        self.crc_rejects += 1;
    }

    pub(crate) fn add_duplicate_crc_reject(&mut self) {
        self.duplicate_crc_rejects += 1;
    }

    pub(crate) fn stats(&self) -> DetectorStats {
        DetectorStats {
            aa_candidates: self.aa_candidates,
            header_rejects: self.header_rejects,
            packets: self.packets,
            crc_rejects: self.crc_rejects,
            duplicate_crc_rejects: self.duplicate_crc_rejects,
            connect_ind_packets: self.connect_ind_packets,
        }
    }

    fn search_access_address(&mut self) -> DetectorEvent {
        let window = self.shift_reg;
        let diff_normal = (window ^ ble_protocol::BLE_ACCESS_ADDRESS).count_ones();
        let diff_invert = (window ^ BLE_ACCESS_ADDRESS_INVERT).count_ones();

        if diff_normal <= MAX_HAMMING_DISTANCE {
            self.aa_candidates += 1;
            if print_diagnostics() {
                println!(
                    "BLE access address caught: phase={} aa=0x{:08X} hamming={}",
                    self.phase,
                    ble_protocol::BLE_ACCESS_ADDRESS,
                    diff_normal
                );
            }
            self.start_packet_recovery(false);
        } else if diff_invert <= MAX_HAMMING_DISTANCE {
            self.aa_candidates += 1;
            if print_diagnostics() {
                println!(
                    "BLE access address caught with inverted phase: phase={} aa=0x{:08X} hamming={}",
                    self.phase,
                    ble_protocol::BLE_ACCESS_ADDRESS,
                    diff_invert
                );
            }
            self.start_packet_recovery(true);
        }

        DetectorEvent::None
    }

    fn capture_candidate_bit(&mut self, bit: u8) -> DetectorEvent {
        self.candidate_bits.push(bit);

        if self.candidate_bits.len() < PDU_OFFSET_COUNT + 16 {
            return DetectorEvent::None;
        }

        match self.try_candidate_offsets() {
            CandidateWindowResult::NeedMoreBits => DetectorEvent::None,
            CandidateWindowResult::HeaderRejected => {
                self.header_rejects += 1;
                if print_diagnostics() {
                    println!(
                        "BLE header rejected: phase={} no plausible offset",
                        self.phase
                    );
                }
                self.reset_to_search();
                DetectorEvent::HeaderRejected
            }
            CandidateWindowResult::CrcRejected { reason } => {
                self.reset_to_search();
                DetectorEvent::CrcRejected { reason }
            }
            CandidateWindowResult::ValidPacket { packet } => {
                self.packets += 1;
                if packet.pdu_type == ble_protocol::BleAdvPduType::ConnectInd {
                    self.connect_ind_packets += 1;
                }
                self.reset_to_search();
                DetectorEvent::ValidPacket { packet }
            }
        }
    }

    fn try_candidate_offsets(&self) -> CandidateWindowResult {
        let mut plausible = 0usize;
        let mut max_needed = 0usize;
        let mut last_error = None;

        for offset in 0..PDU_OFFSET_COUNT {
            let Some(needed_bits) = self.candidate_needed_bits(offset) else {
                continue;
            };

            plausible += 1;
            max_needed = max_needed.max(needed_bits);

            if self.candidate_bits.len() < needed_bits {
                continue;
            }

            let whitened_pdu_crc = bits_to_bytes(&self.candidate_bits[offset..needed_bits]);
            match ble_protocol::parse_advertising_pdu(self.channel_index, &whitened_pdu_crc) {
                Ok(packet) => {
                    if self.print_packets && print_diagnostics() {
                        let relative_offset = offset as isize - PDU_OFFSET_RADIUS_BITS as isize;
                        println!(
                            "BLE packet: phase={} offset={} {}",
                            self.phase,
                            relative_offset,
                            ble_protocol::format_packet_summary(&packet)
                        );
                    } else if self.print_packets {
                        println!(
                            "BLE packet: {}",
                            ble_protocol::format_packet_summary(&packet)
                        );
                    }
                    return CandidateWindowResult::ValidPacket { packet };
                }
                Err(err) => last_error = Some(err.to_string()),
            }
        }

        if plausible == 0 && self.candidate_bits.len() >= PDU_OFFSET_COUNT + 16 {
            return CandidateWindowResult::HeaderRejected;
        }

        if self.candidate_bits.len() >= BLE_CAPTURE_WINDOW_BITS
            || (plausible > 0 && self.candidate_bits.len() >= max_needed)
        {
            return CandidateWindowResult::CrcRejected {
                reason: last_error.unwrap_or_else(|| "candidate window did not pass CRC".into()),
            };
        }

        CandidateWindowResult::NeedMoreBits
    }

    fn candidate_needed_bits(&self, offset: usize) -> Option<usize> {
        if self.candidate_bits.len() < offset + 16 {
            return None;
        }

        let header = bits_to_bytes(&self.candidate_bits[offset..offset + 16]);
        let header = ble_protocol::apply_whitening(self.channel_index, &header).ok()?;
        let payload_len = (header[1] & 0x3f) as usize;

        if payload_len > ble_protocol::BLE_MAX_ADV_PAYLOAD_LEN {
            return None;
        }

        Some(offset + (2 + payload_len + BLE_CRC_LEN) * 8)
    }

    fn start_packet_recovery(&mut self, invert: bool) {
        self.state = PacketState::CapturingCandidateWindow;
        self.invert_signal = invert;
        self.candidate_bits.clear();

        for bit_index in 32 - PDU_OFFSET_RADIUS_BITS..32 {
            let mut bit = ((self.shift_reg >> bit_index) & 1) as u8;
            if invert {
                bit ^= 1;
            }
            self.candidate_bits.push(bit);
        }
    }
}

enum CandidateWindowResult {
    NeedMoreBits,
    HeaderRejected,
    CrcRejected { reason: String },
    ValidPacket { packet: BlePacket },
}

fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0u8, |byte, (bit_index, bit)| {
                    byte | ((*bit & 1) << bit_index)
                })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DetectorStats {
    pub(crate) aa_candidates: u64,
    pub(crate) header_rejects: u64,
    pub(crate) packets: u64,
    pub(crate) crc_rejects: u64,
    pub(crate) duplicate_crc_rejects: u64,
    pub(crate) connect_ind_packets: u64,
}

impl DetectorStats {
    pub(crate) fn pdu_attempts(&self) -> u64 {
        self.packets + self.crc_rejects
    }

    pub(crate) fn aa_per_packet(&self) -> f64 {
        if self.packets == 0 {
            0.0
        } else {
            self.aa_candidates as f64 / self.packets as f64
        }
    }

    pub(crate) fn crc_reject_rate(&self) -> f64 {
        percentage(self.crc_rejects, self.pdu_attempts())
    }

    pub(crate) fn raw_crc_rejects(&self) -> u64 {
        self.crc_rejects + self.duplicate_crc_rejects
    }
}

fn percentage(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::DetectorStats;
    use super::bits_to_bytes;

    #[test]
    fn stats_report_reject_rates() {
        let stats = DetectorStats {
            aa_candidates: 10,
            header_rejects: 2,
            packets: 4,
            crc_rejects: 4,
            duplicate_crc_rejects: 2,
            connect_ind_packets: 1,
        };

        assert_eq!(stats.pdu_attempts(), 8);
        assert_eq!(stats.aa_per_packet(), 2.5);
        assert_eq!(stats.crc_reject_rate(), 50.0);
        assert_eq!(stats.raw_crc_rejects(), 6);
    }

    #[test]
    fn converts_lsb_first_bits_to_bytes() {
        assert_eq!(bits_to_bytes(&[0, 1, 1, 0, 1, 1, 0, 1]), [0xb6]);
    }
}
