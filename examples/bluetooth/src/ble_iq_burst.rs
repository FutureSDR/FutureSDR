use futuresdr::num_complex::Complex32;
use futuresdr::runtime::dev::prelude::*;

use crate::ble_burst_catcher::BurstCatcher;
use crate::ble_burst_catcher::CapturedBurst;
use crate::ble_burst_catcher::PRE_TRIGGER_SYMBOLS;
use crate::ble_burst_decoder::BurstDecodeOutcome;
use crate::ble_burst_decoder::BurstDecodeResult;
use crate::ble_burst_decoder::BurstDecoder;
use crate::ble_burst_decoder::UnsupportedPhy;
use crate::ble_burst_stats::BurstStats;
use crate::ble_connection::SharedConnectionTable;
use crate::ble_connection::format_connection_summary;
use crate::ble_connection_follower::format_planned_events;
use crate::ble_connection_report::format_control_summary;
use crate::ble_connection_report::format_tracking_summary;
use crate::ble_data::BleDataPacket;
use crate::ble_data::format_data_packet_summary;
use crate::ble_protocol::BlePacket;
use crate::ble_protocol::BlePhy;
use crate::ble_wireshark;
use crate::diagnostics;

pub use crate::ble_burst_catcher::SquelchConfig;

const FOLLOW_GUARD_SYMBOLS: u64 = 2_048;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BleIqBurstOptions {
    pub(crate) print_packets: bool,
    pub(crate) wireshark_output: bool,
    pub(crate) follow_connections: bool,
}

#[derive(Block)]
#[message_inputs(rx_overflow)]
#[message_outputs(wireshark)]
pub struct BleIqBurstBlock<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<u8>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = u8>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    channel_index: u8,
    sample_rate_hz: u64,
    print_packets: bool,
    wireshark_output: bool,
    follow_connections: bool,
    decoder: BurstDecoder,
    catcher: BurstCatcher,
    stats: BurstStats,
    connections: SharedConnectionTable,
}

impl<I, O> BleIqBurstBlock<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = u8>,
{
    pub fn with_packet_and_wireshark_output(
        phy: BlePhy,
        samples_per_symbol: usize,
        channel_index: u8,
        filter_taps: Vec<f32>,
        squelch: SquelchConfig,
        options: BleIqBurstOptions,
    ) -> std::result::Result<Self, UnsupportedPhy> {
        let samples_per_symbol = samples_per_symbol.max(1);
        let filter_history = filter_taps.len().saturating_sub(1);
        Ok(Self {
            input: I::default(),
            output: O::default(),
            channel_index,
            sample_rate_hz: phy.symbol_rate_hz() as u64 * samples_per_symbol as u64,
            print_packets: options.print_packets,
            wireshark_output: options.wireshark_output,
            follow_connections: options.follow_connections,
            decoder: BurstDecoder::new(phy, samples_per_symbol, channel_index, filter_taps)?,
            catcher: BurstCatcher::new(
                squelch,
                filter_history + PRE_TRIGGER_SYMBOLS * samples_per_symbol,
            ),
            stats: BurstStats::new(samples_per_symbol),
            connections: SharedConnectionTable::default(),
        })
    }

    pub(crate) fn with_connections(mut self, connections: SharedConnectionTable) -> Self {
        self.connections = connections;
        self
    }

    async fn rx_overflow(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let Pmt::U64(sequence) = p else {
            return Ok(Pmt::InvalidValue);
        };
        self.connections.observe_overflow(sequence);
        Ok(Pmt::Ok)
    }

    fn process_burst(&mut self, burst: CapturedBurst) -> ProcessedBurst {
        let burst_start_sample = burst.start_sample_index;
        self.stats.bursts += 1;
        self.stats.total_burst_samples += burst.samples.len() as u64;
        self.stats.max_burst_samples = self.stats.max_burst_samples.max(burst.samples.len());
        self.stats.forced_closes += u64::from(burst.forced_close);

        let burst_end_sample =
            burst_start_sample.saturating_add(burst.samples.len().saturating_sub(1) as u64);
        let connections = if self.follow_connections {
            let selection = self.connections.decode_contexts_for_window(
                self.channel_index,
                burst_start_sample,
                burst_end_sample,
                FOLLOW_GUARD_SYMBOLS * self.decoder.samples_per_symbol() as u64,
            );
            self.stats.follower_locked_checks += selection.locked_checks;
            self.stats.follower_window_matches += selection.window_matches;
            self.stats.follower_searching_checks += selection.searching_checks;
            selection.contexts
        } else {
            self.connections.decode_contexts()
        };
        if should_gate_data_burst(
            self.follow_connections,
            self.channel_index,
            connections.len(),
        ) {
            self.stats.gated_bursts += 1;
            return ProcessedBurst::default();
        }

        let decode_advertising = !self.follow_connections || self.channel_index >= 37;
        let diagnostics = self.print_packets && diagnostics::enabled();
        let result =
            match self
                .decoder
                .decode(&burst, &connections, decode_advertising, diagnostics)
            {
                BurstDecodeOutcome::Short => {
                    self.stats.short_bursts += 1;
                    return ProcessedBurst::default();
                }
                BurstDecodeOutcome::FskRejected => {
                    self.stats.fsk_rejects += 1;
                    return ProcessedBurst::default();
                }
                BurstDecodeOutcome::Decoded(result) => result,
            };

        let BurstDecodeResult {
            candidates,
            detector_stats,
            phase_valid_packets,
            phase_only_packets,
            phase_duplicates,
            data,
        } = result;

        if self.channel_index >= 37 {
            self.stats
                .observe_crc_burst(!candidates.is_empty(), detector_stats.crc_rejects);
        } else {
            self.stats
                .observe_crc_burst(!data.candidates.is_empty(), data.crc_rejects);
        }

        add_phase_counts(&mut self.stats.phase_valid_packets, &phase_valid_packets);
        add_phase_counts(&mut self.stats.phase_only_packets, &phase_only_packets);
        self.stats.phase_duplicates += phase_duplicates;

        let advertising: Vec<_> = candidates
            .into_iter()
            .map(|candidate| {
                if self.print_packets {
                    if diagnostics {
                        println!(
                            "BLE packet: phase={} sample={} {}",
                            candidate.phase,
                            candidate.sample_index,
                            crate::ble_protocol::format_packet_summary(&candidate.packet)
                        );
                    } else {
                        println!(
                            "BLE packet: {}",
                            crate::ble_protocol::format_packet_summary(&candidate.packet)
                        );
                    }
                }
                self.connections.observe_packet_at(
                    &candidate.packet,
                    burst_start_sample + candidate.sample_index as u64,
                    self.sample_rate_hz,
                );
                candidate.packet
            })
            .collect();

        let data_packets: Vec<_> = data
            .candidates
            .into_iter()
            .map(|candidate| {
                let event = self.connections.observe_data_packet_at(
                    &candidate.packet,
                    burst_start_sample + candidate.sample_index as u64,
                    self.sample_rate_hz,
                );
                let event_summary = event
                    .map(|event| {
                        let event_label = if event.counter_exact {
                            "event"
                        } else {
                            "event_phase"
                        };
                        format!(
                            " {event_label}={} hop_ch={}",
                            event.event_counter, event.channel_index
                        )
                    })
                    .unwrap_or_default();
                if self.print_packets {
                    if diagnostics {
                        println!(
                            "BLE data packet: phase={} sample={} {}{}",
                            candidate.phase,
                            candidate.sample_index,
                            format_data_packet_summary(&candidate.packet),
                            event_summary,
                        );
                    } else {
                        println!(
                            "BLE data packet: {}{}",
                            format_data_packet_summary(&candidate.packet),
                            event_summary,
                        );
                    }
                }
                candidate.packet
            })
            .collect();

        self.stats.detector.merge(detector_stats);
        self.stats.data_packets += data_packets.len() as u64;
        self.stats.data_aa_candidates += data.aa_candidates;
        self.stats.data_crc_rejects += data.crc_rejects;
        self.stats.data_phase_duplicates += data.phase_duplicates;
        let packet_count = advertising.len() + data_packets.len();
        if packet_count > 0 {
            self.stats.decoded_bursts += 1;
        }
        if packet_count > 1 {
            self.stats.multi_packet_bursts += 1;
        }

        ProcessedBurst {
            advertising,
            data: data_packets,
        }
    }

    fn print_stats(&self) {
        let stats = self.stats.detector;
        let diagnostics = self.print_packets && diagnostics::enabled();
        if self.channel_index >= 37 {
            println!(
                "BLE summary: ch={} mode=burst packets={} connect_ind={} crc_passes={} crc_checks={} crc_pass_rate={:.1}%",
                self.channel_index,
                stats.packets,
                stats.connect_ind_packets,
                self.stats.crc_pass_bursts,
                self.stats.burst_crc_checks(),
                self.stats.burst_crc_pass_rate(),
            );
        } else {
            println!(
                "BLE summary: ch={} mode=burst data_packets={} crc_passes={} crc_checks={} crc_pass_rate={:.1}% gated_bursts={}",
                self.channel_index,
                self.stats.data_packets,
                self.stats.crc_pass_bursts,
                self.stats.burst_crc_checks(),
                self.stats.burst_crc_pass_rate(),
                self.stats.gated_bursts,
            );
        }

        if diagnostics && self.channel_index >= 37 {
            println!(
                "BLE detector diagnostics: ch={} aa_candidates={} header_rejects={} valid_phase_candidates={} phase_duplicates={} phase_only_packets={:?} candidate_crc_checks={} candidate_crc_rejects={} candidate_crc_reject_rate={:.1}%",
                self.channel_index,
                stats.aa_candidates,
                stats.header_rejects,
                self.stats.advertising_valid_candidates(),
                self.stats.phase_duplicates,
                self.stats.phase_only_packets,
                self.stats.advertising_candidate_crc_checks(),
                stats.crc_rejects,
                self.stats.advertising_candidate_crc_reject_rate(),
            );
        }
        if diagnostics && self.channel_index < 37 {
            println!(
                "BLE detector diagnostics: ch={} aa_candidates={} valid_phase_candidates={} phase_duplicates={} candidate_crc_checks={} candidate_crc_rejects={} candidate_crc_reject_rate={:.1}%",
                self.channel_index,
                self.stats.data_aa_candidates,
                self.stats.data_valid_candidates(),
                self.stats.data_phase_duplicates,
                self.stats.data_candidate_crc_checks(),
                self.stats.data_crc_rejects,
                self.stats.data_candidate_crc_reject_rate(),
            );
        }
        if diagnostics && self.follow_connections {
            println!(
                "BLE follower diagnostics: ch={} locked_checks={} window_matches={} skipped={} searching_checks={}",
                self.channel_index,
                self.stats.follower_locked_checks,
                self.stats.follower_window_matches,
                self.stats
                    .follower_locked_checks
                    .saturating_sub(self.stats.follower_window_matches),
                self.stats.follower_searching_checks,
            );
        }
        if diagnostics {
            println!(
                "BLE burst diagnostics: ch={} bursts={} gated_bursts={} decoded_bursts={} short_bursts={} fsk_rejects={} mean_samples={:.1} max_samples={} forced_closes={} multi_packet_bursts={} packets_per_burst={:.2}",
                self.channel_index,
                self.stats.bursts,
                self.stats.gated_bursts,
                self.stats.decoded_bursts,
                self.stats.short_bursts,
                self.stats.fsk_rejects,
                self.stats.mean_burst_samples(),
                self.stats.max_burst_samples,
                self.stats.forced_closes,
                self.stats.multi_packet_bursts,
                self.stats.packets_per_burst(),
            );
            println!(
                "BLE squelch diagnostics: ch={} {}",
                self.channel_index,
                self.catcher.summary()
            );
        }

        if self.connections.finish_reporter() {
            if diagnostics {
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
            if self.follow_connections || summary.connect_ind_seen > 0 {
                println!("BLE tracking summary: {}", format_tracking_summary(summary));
            }
            if let Some(control) = format_control_summary(summary) {
                println!("BLE control summary: {control}");
            }
        }
    }
}

impl<I, O> Kernel for BleIqBurstBlock<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
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

        let mut bursts = Vec::new();
        {
            let input_samples = &self.input.slice()[..n];
            let catcher = &mut self.catcher;
            for &sample in input_samples {
                if let Some(burst) = catcher.execute(sample) {
                    bursts.push(burst);
                }
            }
        }

        self.input.consume(n);
        for burst in bursts {
            let packets = self.process_burst(burst);
            if self.wireshark_output {
                post_wireshark_packets(_mo, &packets).await?;
            }
        }

        if self.input.finished() {
            _io.finished = true;
        }
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        if let Some(burst) = self.catcher.finish(false) {
            let packets = self.process_burst(burst);
            if self.wireshark_output {
                post_wireshark_packets(_mo, &packets).await?;
            }
        }
        if self.wireshark_output {
            _mo.post("wireshark", Pmt::Finished).await?;
        }
        self.print_stats();
        Ok(())
    }
}

#[derive(Default)]
struct ProcessedBurst {
    advertising: Vec<BlePacket>,
    data: Vec<BleDataPacket>,
}

async fn post_wireshark_packets(mo: &mut MessageOutputs, packets: &ProcessedBurst) -> Result<()> {
    for packet in &packets.advertising {
        mo.post(
            "wireshark",
            Pmt::Blob(ble_wireshark::packet_to_udp_payload(packet)),
        )
        .await?;
    }
    for packet in &packets.data {
        mo.post(
            "wireshark",
            Pmt::Blob(ble_wireshark::data_packet_to_udp_payload(packet)),
        )
        .await?;
    }

    Ok(())
}

fn should_gate_data_burst(
    follow_connections: bool,
    channel_index: u8,
    connection_count: usize,
) -> bool {
    follow_connections && channel_index < 37 && connection_count == 0
}

fn add_phase_counts(accumulator: &mut [u64], counts: &[u64]) {
    for (accumulator, count) in accumulator.iter_mut().zip(counts) {
        *accumulator += count;
    }
}

#[cfg(test)]
mod tests {
    use super::should_gate_data_burst;

    #[test]
    fn gates_unmatched_follower_data_bursts() {
        assert!(should_gate_data_burst(true, 0, 0));
        assert!(!should_gate_data_burst(true, 0, 1));
        assert!(!should_gate_data_burst(true, 37, 0));
        assert!(!should_gate_data_burst(false, 0, 0));
    }
}
