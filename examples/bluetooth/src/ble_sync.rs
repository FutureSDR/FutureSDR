use futuresdr::runtime::dev::prelude::*;

use crate::ble_connection::SharedConnectionTable;
use crate::ble_connection::format_connection_summary;
use crate::ble_connection_follower::format_planned_events;
use crate::ble_connection_report::format_tracking_summary;
use crate::ble_detector::DetectorEvent;
use crate::ble_detector::DetectorStats;
use crate::ble_detector::PacketDetector;
use crate::ble_slicer::SymbolSlicer;
use crate::ble_wireshark;

const CRC_REJECT_GUARD_SYMBOLS: usize = 512;

use crate::diagnostics;

struct PendingReject {
    sample_index: usize,
    phase: usize,
    reason: String,
}

#[derive(Block)]
#[message_outputs(wireshark)]
pub struct BleSyncBlock<I = DefaultCpuReader<f32>, O = DefaultCpuWriter<u8>>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    channel_index: u8,
    samples_per_symbol: usize,
    initial_delay: usize,
    total_sample_index: usize,
    print_packets: bool,
    wireshark_output: bool,
    slicer: SymbolSlicer,
    detectors: Vec<PacketDetector>,
    pending_crc_rejects: Vec<PendingReject>,
    connections: SharedConnectionTable,
}

impl<I, O> BleSyncBlock<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    #[cfg(test)]
    fn with_channel_and_phase(
        samples_per_symbol: usize,
        channel_index: u8,
        initial_delay: usize,
    ) -> Self {
        Self::with_channel_phase_packet_and_wireshark_output(
            samples_per_symbol,
            channel_index,
            initial_delay,
            true,
            false,
        )
    }

    pub fn with_channel_phase_packet_and_wireshark_output(
        samples_per_symbol: usize,
        channel_index: u8,
        initial_delay: usize,
        print_packets: bool,
        wireshark_output: bool,
    ) -> Self {
        let samples_per_symbol = std::cmp::max(1, samples_per_symbol);
        let detectors = (0..samples_per_symbol)
            .map(|phase| PacketDetector::with_packet_output(phase, channel_index, print_packets))
            .collect();

        Self {
            input: I::default(),
            output: O::default(),
            channel_index,
            samples_per_symbol,
            initial_delay,
            total_sample_index: 0,
            print_packets,
            wireshark_output,
            slicer: SymbolSlicer::new(),
            detectors,
            pending_crc_rejects: Vec::new(),
            connections: SharedConnectionTable::default(),
        }
    }

    pub(crate) fn with_connections(mut self, connections: SharedConnectionTable) -> Self {
        self.connections = connections;
        self
    }

    fn aggregate_stats(&self) -> DetectorStats {
        aggregate_stats(self.detectors.iter().map(PacketDetector::stats))
    }

    fn print_stats(&self) {
        let stats = self.aggregate_stats();
        println!(
            "BLE summary: ch={} mode=continuous packets={} connect_ind={} crc_passes={} crc_checks={} crc_pass_rate={:.1}%",
            self.channel_index,
            stats.packets,
            stats.connect_ind_packets,
            stats.packets,
            stats.pdu_attempts(),
            stats.crc_pass_rate()
        );
        if self.print_packets && diagnostics::enabled() {
            println!(
                "BLE detector diagnostics: ch={} aa_candidates={} aa_per_packet={:.2} header_rejects={} duplicate_crc_rejects={} raw_crc_rejects={}",
                self.channel_index,
                stats.aa_candidates,
                stats.aa_per_packet(),
                stats.header_rejects,
                stats.duplicate_crc_rejects,
                stats.raw_crc_rejects(),
            );
        }
        if self.connections.finish_reporter() {
            if self.print_packets && diagnostics::enabled() {
                for connection in self.connections.snapshot() {
                    println!(
                        "BLE connection: ch={} {}",
                        connection.first_channel,
                        format_connection_summary(&connection)
                    );
                }
                for plan in self.connections.follow_plans(8) {
                    println!(
                        "BLE follow plan: aa=0x{:08X} next_events=[{}]",
                        plan.access_address,
                        format_planned_events(&plan.events)
                    );
                }
            }
            let summary = self.connections.tracking_summary();
            if summary.connect_ind_seen > 0 {
                println!("BLE tracking summary: {}", format_tracking_summary(summary));
            }
        }
    }

    fn crc_reject_guard_samples(&self) -> usize {
        self.samples_per_symbol
            .saturating_mul(CRC_REJECT_GUARD_SYMBOLS)
    }

    fn record_crc_reject(&mut self, phase: usize, reason: String) {
        self.pending_crc_rejects.push(PendingReject {
            sample_index: self.total_sample_index,
            phase,
            reason,
        });
    }

    fn suppress_duplicate_crc_rejects(&mut self) {
        let guard_samples = self.crc_reject_guard_samples();
        let now = self.total_sample_index;
        let mut kept = Vec::with_capacity(self.pending_crc_rejects.len());

        for reject in self.pending_crc_rejects.drain(..) {
            if now.saturating_sub(reject.sample_index) <= guard_samples {
                self.detectors[reject.phase].add_duplicate_crc_reject();
            } else {
                kept.push(reject);
            }
        }

        self.pending_crc_rejects = kept;
    }

    fn flush_expired_crc_rejects(&mut self) {
        self.flush_crc_rejects(false);
    }

    fn flush_all_crc_rejects(&mut self) {
        self.flush_crc_rejects(true);
    }

    fn flush_crc_rejects(&mut self, force: bool) {
        let guard_samples = self.crc_reject_guard_samples();
        let now = self.total_sample_index;
        let pending = std::mem::take(&mut self.pending_crc_rejects);
        let mut pending = pending.into_iter().peekable();

        while let Some(reject) = pending.next() {
            if !force && now.saturating_sub(reject.sample_index) <= guard_samples {
                self.pending_crc_rejects.push(reject);
                self.pending_crc_rejects.extend(pending);
                break;
            }

            let window_start = reject.sample_index;
            let phase = reject.phase;
            let reason = reject.reason;
            let mut duplicates = 0u64;

            while pending
                .peek()
                .is_some_and(|next| next.sample_index.saturating_sub(window_start) <= guard_samples)
            {
                let duplicate = pending.next().expect("peeked pending reject");
                self.detectors[duplicate.phase].add_duplicate_crc_reject();
                duplicates += 1;
            }

            self.detectors[phase].add_crc_reject();
            if self.print_packets && diagnostics::enabled() {
                if duplicates == 0 {
                    println!("BLE candidate rejected: phase={phase} {reason}");
                } else {
                    println!(
                        "BLE candidate window rejected: phase={phase} duplicate_rejects={duplicates} {reason}"
                    );
                }
            }
        }
    }
}

impl<I, O> Kernel for BleSyncBlock<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let n = self.input.slice().len();

        if n == 0 {
            if self.input.finished() {
                _io.finished = true;
            }
            return Ok(());
        }

        let input_samples = self.input.slice()[..n].to_vec();

        for sample in input_samples {
            if self.initial_delay > 0 {
                self.initial_delay -= 1;
                self.total_sample_index += 1;
                continue;
            }

            let phase = self.total_sample_index % self.samples_per_symbol;
            let bit = self.slicer.slice(sample);
            match self.detectors[phase].process_symbol(bit) {
                DetectorEvent::None | DetectorEvent::HeaderRejected => {}
                DetectorEvent::ValidPacket { packet } => {
                    self.connections.observe_packet(&packet);
                    if self.wireshark_output {
                        _mo.post(
                            "wireshark",
                            Pmt::Blob(ble_wireshark::packet_to_udp_payload(&packet)),
                        )
                        .await?;
                    }
                    self.suppress_duplicate_crc_rejects();
                    for detector in &mut self.detectors {
                        detector.reset_to_search();
                    }
                }
                DetectorEvent::CrcRejected { reason } => {
                    self.record_crc_reject(phase, reason);
                }
            }

            self.flush_expired_crc_rejects();
            self.total_sample_index += 1;
        }

        self.input.consume(n);
        if self.input.finished() {
            _io.finished = true;
        }
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.flush_all_crc_rejects();
        if self.wireshark_output {
            _mo.post("wireshark", Pmt::Finished).await?;
        }
        self.print_stats();

        Ok(())
    }
}

fn aggregate_stats(stats: impl Iterator<Item = DetectorStats>) -> DetectorStats {
    stats.fold(DetectorStats::default(), |acc, stats| DetectorStats {
        aa_candidates: acc.aa_candidates + stats.aa_candidates,
        header_rejects: acc.header_rejects + stats.header_rejects,
        packets: acc.packets + stats.packets,
        crc_rejects: acc.crc_rejects + stats.crc_rejects,
        duplicate_crc_rejects: acc.duplicate_crc_rejects + stats.duplicate_crc_rejects,
        connect_ind_packets: acc.connect_ind_packets + stats.connect_ind_packets,
    })
}

#[cfg(test)]
mod tests {
    use super::BleSyncBlock;
    use super::DefaultCpuReader;
    use super::DefaultCpuWriter;

    #[test]
    fn crc_guard_marks_nearby_reject_as_duplicate() {
        let mut block =
            BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_and_phase(
                2, 37, 0,
            );

        block.total_sample_index = 100;
        block.record_crc_reject(0, "invalid BLE CRC".to_string());
        block.total_sample_index += block.crc_reject_guard_samples();
        block.suppress_duplicate_crc_rejects();

        assert_eq!(block.detectors[0].stats().crc_rejects, 0);
        assert_eq!(block.detectors[0].stats().duplicate_crc_rejects, 1);
        assert!(block.pending_crc_rejects.is_empty());
    }

    #[test]
    fn crc_guard_flushes_expired_reject() {
        let mut block =
            BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_and_phase(
                2, 37, 0,
            );

        block.total_sample_index = 100;
        block.record_crc_reject(0, "invalid BLE CRC".to_string());
        block.total_sample_index += block.crc_reject_guard_samples() + 1;
        block.flush_expired_crc_rejects();

        assert_eq!(block.detectors[0].stats().crc_rejects, 1);
        assert_eq!(block.detectors[0].stats().duplicate_crc_rejects, 0);
        assert!(block.pending_crc_rejects.is_empty());
    }

    #[test]
    fn crc_guard_groups_nearby_expired_rejects() {
        let mut block =
            BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_and_phase(
                2, 37, 0,
            );

        block.total_sample_index = 100;
        block.record_crc_reject(0, "first invalid BLE CRC".to_string());
        block.total_sample_index += 10;
        block.record_crc_reject(1, "second invalid BLE CRC".to_string());
        block.total_sample_index = 100 + block.crc_reject_guard_samples() + 1;
        block.flush_expired_crc_rejects();

        assert_eq!(block.detectors[0].stats().crc_rejects, 1);
        assert_eq!(block.detectors[1].stats().crc_rejects, 0);
        assert_eq!(block.detectors[1].stats().duplicate_crc_rejects, 1);
        assert!(block.pending_crc_rejects.is_empty());
    }
}
