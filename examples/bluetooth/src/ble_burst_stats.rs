use crate::ble_detector::DetectorStats;

pub(crate) struct BurstStats {
    pub(crate) bursts: u64,
    pub(crate) gated_bursts: u64,
    pub(crate) decoded_bursts: u64,
    pub(crate) short_bursts: u64,
    pub(crate) fsk_rejects: u64,
    pub(crate) total_burst_samples: u64,
    pub(crate) max_burst_samples: usize,
    pub(crate) forced_closes: u64,
    pub(crate) multi_packet_bursts: u64,
    pub(crate) phase_duplicates: u64,
    pub(crate) crc_pass_bursts: u64,
    crc_fail_bursts: u64,
    pub(crate) data_packets: u64,
    pub(crate) data_aa_candidates: u64,
    pub(crate) data_crc_rejects: u64,
    pub(crate) data_phase_duplicates: u64,
    pub(crate) follower_locked_checks: u64,
    pub(crate) follower_window_matches: u64,
    pub(crate) follower_searching_checks: u64,
    pub(crate) phase_valid_packets: Vec<u64>,
    pub(crate) phase_only_packets: Vec<u64>,
    pub(crate) detector: DetectorStats,
}

impl BurstStats {
    pub(crate) fn new(samples_per_symbol: usize) -> Self {
        Self {
            bursts: 0,
            gated_bursts: 0,
            decoded_bursts: 0,
            short_bursts: 0,
            fsk_rejects: 0,
            total_burst_samples: 0,
            max_burst_samples: 0,
            forced_closes: 0,
            multi_packet_bursts: 0,
            phase_duplicates: 0,
            crc_pass_bursts: 0,
            crc_fail_bursts: 0,
            data_packets: 0,
            data_aa_candidates: 0,
            data_crc_rejects: 0,
            data_phase_duplicates: 0,
            follower_locked_checks: 0,
            follower_window_matches: 0,
            follower_searching_checks: 0,
            phase_valid_packets: vec![0; samples_per_symbol],
            phase_only_packets: vec![0; samples_per_symbol],
            detector: DetectorStats::default(),
        }
    }

    pub(crate) fn mean_burst_samples(&self) -> f64 {
        ratio(self.total_burst_samples, self.bursts)
    }

    pub(crate) fn packets_per_burst(&self) -> f64 {
        ratio(self.detector.packets + self.data_packets, self.bursts)
    }

    pub(crate) fn advertising_valid_candidates(&self) -> u64 {
        self.phase_valid_packets.iter().sum()
    }

    pub(crate) fn advertising_candidate_crc_checks(&self) -> u64 {
        self.advertising_valid_candidates() + self.detector.crc_rejects
    }

    pub(crate) fn advertising_candidate_crc_reject_rate(&self) -> f64 {
        percentage(
            self.detector.crc_rejects,
            self.advertising_candidate_crc_checks(),
        )
    }

    pub(crate) fn data_valid_candidates(&self) -> u64 {
        self.data_packets + self.data_phase_duplicates
    }

    pub(crate) fn data_candidate_crc_checks(&self) -> u64 {
        self.data_valid_candidates() + self.data_crc_rejects
    }

    pub(crate) fn data_candidate_crc_reject_rate(&self) -> f64 {
        percentage(self.data_crc_rejects, self.data_candidate_crc_checks())
    }

    pub(crate) fn observe_crc_burst(&mut self, has_valid_packet: bool, crc_rejects: u64) {
        if has_valid_packet {
            self.crc_pass_bursts += 1;
        } else if crc_rejects > 0 {
            self.crc_fail_bursts += 1;
        }
    }

    pub(crate) fn burst_crc_checks(&self) -> u64 {
        self.crc_pass_bursts + self.crc_fail_bursts
    }

    pub(crate) fn burst_crc_pass_rate(&self) -> f64 {
        percentage(self.crc_pass_bursts, self.burst_crc_checks())
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn percentage(numerator: u64, denominator: u64) -> f64 {
    100.0 * ratio(numerator, denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_rates_use_phase_consistent_candidate_counts() {
        let mut stats = BurstStats::new(2);
        stats.phase_valid_packets = vec![40, 40];
        stats.detector.crc_rejects = 20;
        stats.data_packets = 30;
        stats.data_phase_duplicates = 10;
        stats.data_crc_rejects = 10;
        stats.observe_crc_burst(true, 3);
        stats.observe_crc_burst(false, 2);
        stats.observe_crc_burst(false, 0);

        assert_eq!(stats.advertising_candidate_crc_checks(), 100);
        assert_eq!(stats.advertising_candidate_crc_reject_rate(), 20.0);
        assert_eq!(stats.data_candidate_crc_checks(), 50);
        assert_eq!(stats.data_candidate_crc_reject_rate(), 20.0);
        assert_eq!(stats.burst_crc_checks(), 2);
        assert_eq!(stats.burst_crc_pass_rate(), 50.0);
    }
}
