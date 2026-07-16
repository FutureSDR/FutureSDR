use futuresdr::num_complex::Complex32;
use futuresdr::runtime::dev::prelude::*;

use crate::ble_connection::ConnectionTable;
use crate::ble_connection::format_connection_summary;
use crate::ble_detector::DetectorEvent;
use crate::ble_detector::DetectorStats;
use crate::ble_detector::PacketDetector;
use crate::ble_protocol::BlePacket;
use crate::ble_wireshark;

const AGC_ALPHA: f32 = 0.25;
const SQUELCH_TIMEOUT_SAMPLES: usize = 100;
const BURST_START_CAPACITY: usize = 2048;
const MIN_BURST_SAMPLES: usize = 96;
const MAX_BURST_SAMPLES: usize = 8192;
const CFO_MEDIAN_SYMBOLS: usize = 64;
const MAX_FREQ_OFFSET: f32 = 0.85;

fn print_diagnostics() -> bool {
    cfg!(debug_assertions)
}

#[derive(Block)]
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
    samples_per_symbol: usize,
    channel_index: u8,
    print_packets: bool,
    wireshark_output: bool,
    catcher: BurstCatcher,
    stats: BurstStats,
    connections: ConnectionTable,
}

impl<I, O> BleIqBurstBlock<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = u8>,
{
    pub fn with_packet_and_wireshark_output(
        samples_per_symbol: usize,
        channel_index: u8,
        squelch_db: f32,
        print_packets: bool,
        wireshark_output: bool,
    ) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            samples_per_symbol: samples_per_symbol.max(1),
            channel_index,
            print_packets,
            wireshark_output,
            catcher: BurstCatcher::new(squelch_db),
            stats: BurstStats::default(),
            connections: ConnectionTable::default(),
        }
    }

    fn process_burst(&mut self, burst: Vec<Complex32>) -> Vec<BlePacket> {
        self.stats.bursts += 1;

        if burst.len() < MIN_BURST_SAMPLES {
            self.stats.short_bursts += 1;
            return Vec::new();
        }

        let Some(demod) = demodulate_burst(&burst, self.samples_per_symbol) else {
            self.stats.fsk_rejects += 1;
            return Vec::new();
        };

        let mut burst_stats = DetectorStats::default();
        let mut decoded = false;
        let mut packets = Vec::new();

        for phase in 0..self.samples_per_symbol {
            let mut detector =
                PacketDetector::with_packet_output(phase, self.channel_index, self.print_packets);

            for value in demod.iter().skip(phase).step_by(self.samples_per_symbol) {
                match detector.process_symbol(u8::from(*value > 0.0)) {
                    DetectorEvent::None | DetectorEvent::HeaderRejected => {}
                    DetectorEvent::ValidPacket { packet } => {
                        self.connections.observe_packet(&packet);
                        packets.push(packet);
                        decoded = true;
                        break;
                    }
                    DetectorEvent::CrcRejected { reason } => {
                        detector.add_crc_reject();
                        if print_diagnostics() {
                            println!("BLE burst candidate rejected: phase={phase} {reason}");
                        }
                    }
                }
            }

            add_stats(&mut burst_stats, detector.stats());

            if decoded {
                break;
            }
        }

        add_stats(&mut self.stats.detector, burst_stats);
        if decoded {
            self.stats.decoded_bursts += 1;
        }

        packets
    }

    fn print_stats(&self) {
        let stats = self.stats.detector;
        println!(
            "BLE burst stats: ch={} bursts={} decoded_bursts={} short_bursts={} fsk_rejects={} packets={} connect_ind={} connections={} aa_candidates={} aa_per_packet={:.2} header_rejects={} pdu_attempts={} crc_rejects={} crc_reject_rate={:.1}%",
            self.channel_index,
            self.stats.bursts,
            self.stats.decoded_bursts,
            self.stats.short_bursts,
            self.stats.fsk_rejects,
            stats.packets,
            stats.connect_ind_packets,
            self.connections.len(),
            stats.aa_candidates,
            stats.aa_per_packet(),
            stats.header_rejects,
            stats.pdu_attempts(),
            stats.crc_rejects,
            stats.crc_reject_rate(),
        );
        for connection in self.connections.iter() {
            println!(
                "BLE connection: ch={} {}",
                self.channel_index,
                format_connection_summary(connection)
            );
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
        if let Some(burst) = self.catcher.finish() {
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

async fn post_wireshark_packets(mo: &mut MessageOutputs, packets: &[BlePacket]) -> Result<()> {
    for packet in packets {
        mo.post(
            "wireshark",
            Pmt::Blob(ble_wireshark::packet_to_udp_payload(packet)),
        )
        .await?;
    }

    Ok(())
}

#[derive(Default)]
struct BurstStats {
    bursts: u64,
    decoded_bursts: u64,
    short_bursts: u64,
    fsk_rejects: u64,
    detector: DetectorStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SquelchState {
    SignalLo,
    Rise,
    SignalHi,
    Timeout,
}

struct BurstCatcher {
    gain: f32,
    signal_level: f32,
    squelch_db: f32,
    state: SquelchState,
    timer: usize,
    burst_buf: Vec<Complex32>,
    capturing: bool,
}

impl BurstCatcher {
    fn new(squelch_db: f32) -> Self {
        Self {
            gain: 1000.0,
            signal_level: 1e-3,
            squelch_db,
            state: SquelchState::SignalLo,
            timer: 0,
            burst_buf: Vec::new(),
            capturing: false,
        }
    }

    fn execute(&mut self, sample: Complex32) -> Option<Vec<Complex32>> {
        let (sample, state) = self.agc_and_squelch(sample);

        match state {
            SquelchState::Rise => {
                self.burst_buf = Vec::with_capacity(BURST_START_CAPACITY);
                self.burst_buf.push(sample);
                self.capturing = true;
                None
            }
            SquelchState::SignalHi => {
                if self.capturing && self.burst_buf.len() < MAX_BURST_SAMPLES {
                    self.burst_buf.push(sample);
                }
                None
            }
            SquelchState::Timeout => self.finish(),
            SquelchState::SignalLo => None,
        }
    }

    fn finish(&mut self) -> Option<Vec<Complex32>> {
        if self.capturing && !self.burst_buf.is_empty() {
            self.capturing = false;
            Some(std::mem::take(&mut self.burst_buf))
        } else {
            self.capturing = false;
            None
        }
    }

    fn agc_and_squelch(&mut self, input: Complex32) -> (Complex32, SquelchState) {
        let output = input * self.gain;
        let level = output.norm();
        self.signal_level = AGC_ALPHA * level + (1.0 - AGC_ALPHA) * self.signal_level;

        if self.signal_level > 1e-6 {
            self.gain *= (-0.5 * AGC_ALPHA * self.signal_level.ln()).exp();
            self.gain = self.gain.clamp(1e-6, 1e6);
        }

        let rssi_db = -20.0 * self.gain.log10();
        let above_threshold = rssi_db > self.squelch_db;

        self.state = match self.state {
            SquelchState::SignalLo => {
                if above_threshold {
                    SquelchState::Rise
                } else {
                    SquelchState::SignalLo
                }
            }
            SquelchState::Rise => {
                if above_threshold {
                    self.timer = 0;
                    SquelchState::SignalHi
                } else {
                    self.timer = 0;
                    SquelchState::SignalLo
                }
            }
            SquelchState::SignalHi => {
                if above_threshold {
                    self.timer = 0;
                    SquelchState::SignalHi
                } else {
                    self.timer += 1;
                    if self.timer >= SQUELCH_TIMEOUT_SAMPLES {
                        SquelchState::Timeout
                    } else {
                        SquelchState::SignalHi
                    }
                }
            }
            SquelchState::Timeout => {
                if above_threshold {
                    SquelchState::Rise
                } else {
                    SquelchState::SignalLo
                }
            }
        };

        (output, self.state)
    }
}

fn demodulate_burst(samples: &[Complex32], samples_per_symbol: usize) -> Option<Vec<f32>> {
    if samples.len() < 8 + samples_per_symbol * CFO_MEDIAN_SYMBOLS {
        return None;
    }

    let mut demod = Vec::with_capacity(samples.len().saturating_sub(1));
    let mut prev = samples[0];
    for &sample in &samples[1..] {
        demod.push((sample * prev.conj()).arg() * std::f32::consts::FRAC_1_PI);
        prev = sample;
    }

    let (cfo, deviation) = estimate_cfo_and_deviation(&demod, samples_per_symbol)?;
    for value in &mut demod {
        *value = (*value - cfo) / deviation;
    }

    Some(demod)
}

fn estimate_cfo_and_deviation(demod: &[f32], samples_per_symbol: usize) -> Option<(f32, f32)> {
    let end = 8 + samples_per_symbol * CFO_MEDIAN_SYMBOLS;
    if end > demod.len() {
        return None;
    }

    let mut pos = Vec::with_capacity(samples_per_symbol * CFO_MEDIAN_SYMBOLS);
    let mut neg = Vec::with_capacity(samples_per_symbol * CFO_MEDIAN_SYMBOLS);

    for &value in &demod[8..end] {
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

fn add_stats(acc: &mut DetectorStats, stats: DetectorStats) {
    acc.aa_candidates += stats.aa_candidates;
    acc.header_rejects += stats.header_rejects;
    acc.packets += stats.packets;
    acc.crc_rejects += stats.crc_rejects;
    acc.duplicate_crc_rejects += stats.duplicate_crc_rejects;
    acc.connect_ind_packets += stats.connect_ind_packets;
}

#[cfg(test)]
mod tests {
    use super::demodulate_burst;
    use crate::ble_detector::DetectorEvent;
    use crate::ble_detector::PacketDetector;
    use crate::ble_sim::generate_ble_packet_samples;

    #[test]
    fn demodulates_simulated_burst() {
        let samples = generate_ble_packet_samples(2, 1, 37);
        let burst: Vec<_> = samples
            .into_iter()
            .filter(|sample| sample.norm() > 0.0)
            .collect();
        let demod = demodulate_burst(&burst, 2).unwrap();
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
}
