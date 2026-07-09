use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use futuredsp::firdes::remez;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::PfbChannelizer;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::seify::Builder;
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::runtime::Error as RuntimeError;
use futuresdr::runtime::dev::DefaultCpuReader;
use futuresdr::runtime::dev::DefaultCpuWriter;
use std::sync::mpsc;
use std::time::Duration;

const BLE_CHANNEL_SPACING_HZ: f64 = 2.0e6;
const MULTI_CHANNEL_GUARD_BINS: usize = 1;

mod ble_ad;
mod ble_connection;
mod ble_detector;
mod ble_iq_burst;
mod ble_phy;
mod ble_protocol;
mod ble_sim;
mod ble_slicer;
mod ble_sync;
mod gmsk_demod;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    #[arg(short, long, default_value = "")]
    args: String,
    #[arg(short, long, default_value_t = 37)]
    channel: u8,
    #[arg(long, value_delimiter = ',')]
    channels: Vec<u8>,
    #[arg(long, value_delimiter = ',')]
    passive_channels: Vec<u8>,
    #[arg(long, default_value_t = false)]
    multi_channel: bool,
    #[arg(short, long, default_value_t = 2.0e6)]
    sample_rate: f64,
    #[arg(short, long, default_value_t = 30.0)]
    gain: f64,
    #[arg(long)]
    antenna: Option<String>,
    #[arg(long, default_value_t = 0.0)]
    threshold: f32,
    #[arg(long, default_value_t = false)]
    normalize_levels: bool,
    #[arg(long, default_value_t = false)]
    burst: bool,
    #[arg(long, default_value_t = -50.0)]
    squelch_db: f32,
    #[arg(long, default_value_t = false)]
    quiet: bool,
    #[arg(long, default_value_t = false)]
    simulate: bool,
    #[arg(long, default_value_t = 1)]
    simulate_packets: usize,
    #[arg(long)]
    duration: Option<f64>,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();

    println!("Starting Bluetooth GMSK sniffer example");
    let mut fg = Flowgraph::new();
    let selected_channels = selected_channels(&args)?;
    let capture_channels = capture_channels(&args, &selected_channels)?;
    let mut sample_rate = args.sample_rate;

    if args.multi_channel && (sample_rate - 2.0e6).abs() < f64::EPSILON {
        let center_frequency = center_frequency_for_channels(&capture_channels)?;
        sample_rate = auto_multi_channel_sample_rate(&capture_channels, center_frequency)?;
        println!(
            "Auto-selected {:.3} MS/s for multi-channel capture. Override with --sample-rate if needed.",
            sample_rate / 1.0e6
        );
    }

    if args.multi_channel {
        if args.simulate {
            bail!("--multi-channel simulation is not implemented yet");
        }
        build_multi_channel_flowgraph(
            &mut fg,
            &args,
            sample_rate,
            &selected_channels,
            &capture_channels,
            args.burst,
        )?;
    } else {
        build_single_channel_flowgraph(&mut fg, &args, sample_rate)?;
    }

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    println!("Runtime started.");

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = shutdown_tx.send(());
    })?;

    if args.simulate && args.duration.is_none() {
        Runtime::block_on(async move {
            match running.wait_async().await {
                Ok(_) | Err(RuntimeError::FlowgraphTerminated) => {}
                Err(err) => return Err(err.into()),
            }
            Ok::<(), anyhow::Error>(())
        })?;
    } else {
        if let Some(duration) = args.duration {
            let _ = shutdown_rx.recv_timeout(Duration::from_secs_f64(duration));
        } else {
            println!("Press Ctrl+C to stop.");
            shutdown_rx.recv()?;
        }

        Runtime::block_on(async move {
            match running.stop().await {
                Ok(()) | Err(RuntimeError::FlowgraphTerminated) => {}
                Err(err) => return Err(err.into()),
            }
            match running.wait_async().await {
                Ok(_) | Err(RuntimeError::FlowgraphTerminated) => {}
                Err(err) => return Err(err.into()),
            }
            Ok::<(), anyhow::Error>(())
        })?;
    }

    println!("Runtime stopped.");
    Ok(())
}

fn build_single_channel_flowgraph(
    mut fg: &mut Flowgraph,
    args: &Args,
    sample_rate: f64,
) -> Result<()> {
    let samples_per_symbol = (sample_rate / 1e6).round().max(1.0) as usize;
    let symbol_rate_ratio = sample_rate / 1e6;
    if (symbol_rate_ratio - samples_per_symbol as f64).abs() > 0.05 {
        println!(
            "Sample rate {:.3} MS/s is not close to an integer multiple of the BLE 1 Msym/s symbol rate; symbol slicing may be less reliable",
            sample_rate / 1.0e6
        );
    }

    let channel_index = args.channel;
    let frequency = ble_protocol::frequency_hz_from_channel_index(channel_index)?;
    println!(
        "Using BLE channel {channel_index} ({:.3} MHz)",
        frequency / 1.0e6
    );
    if args.burst {
        println!(
            "Using IQ burst decoder with squelch threshold {:.1} dB.",
            args.squelch_db
        );
    } else {
        println!("Using continuous packet decoder.");
    }

    let rx_taps = gmsk_demod::gmsk_rx_taps(samples_per_symbol)?;
    println!("GMSK receive filter configured.");
    let gmsk_filter = FirBuilder::fir::<Complex32, Complex32, _>(rx_taps);
    let snk = NullSink::<u8>::new();

    if args.burst {
        let ble_burst = ble_iq_burst::BleIqBurstBlock::<
            DefaultCpuReader<Complex32>,
            DefaultCpuWriter<u8>,
        >::with_packet_output(
            samples_per_symbol,
            channel_index,
            args.squelch_db,
            !args.quiet,
        );

        if args.simulate {
            println!("Simulation mode: Testing full GMSK demodulator DSP chain");
            let simulated_samples = ble_sim::generate_ble_packet_samples(
                samples_per_symbol,
                args.simulate_packets,
                channel_index,
            );
            let src = VectorSource::<Complex32>::new(simulated_samples);
            connect!(fg, src > gmsk_filter > ble_burst > snk);
        } else {
            println!("Hardware mode: Deploying Seify source live SDR chain");
            let src = Builder::new(args.args.clone())?
                .frequency(frequency)
                .sample_rate(sample_rate)
                .gain(args.gain)
                .antenna(args.antenna.clone())
                .build_source()?;
            connect!(fg, src.outputs[0] > gmsk_filter > ble_burst > snk);
        }
    } else {
        let ble_sync = ble_sync::BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_phase_and_packet_output(
            args.threshold,
            args.normalize_levels,
            samples_per_symbol,
            channel_index,
            0,
            !args.quiet,
        );
        let discriminator = discriminator_block();

        if args.simulate {
            println!("Simulation mode: Testing full GMSK demodulator DSP chain");
            let simulated_samples = ble_sim::generate_ble_packet_samples(
                samples_per_symbol,
                args.simulate_packets,
                channel_index,
            );
            let src = VectorSource::<Complex32>::new(simulated_samples);
            connect!(fg, src > gmsk_filter > discriminator > ble_sync > snk);
        } else {
            println!("Hardware mode: Deploying Seify source live SDR chain");
            let src = Builder::new(args.args.clone())?
                .frequency(frequency)
                .sample_rate(sample_rate)
                .gain(args.gain)
                .antenna(args.antenna.clone())
                .build_source()?;
            connect!(fg, src.outputs[0] > gmsk_filter > discriminator > ble_sync > snk);
        }
    }

    Ok(())
}

fn selected_channels(args: &Args) -> Result<Vec<u8>> {
    let mut channels = if args.channels.is_empty() {
        vec![args.channel]
    } else {
        args.channels.clone()
    };

    channels.sort_unstable();
    channels.dedup();

    for &channel in &channels {
        ble_protocol::frequency_hz_from_channel_index(channel)?;
    }

    Ok(channels)
}

fn capture_channels(args: &Args, selected_channels: &[u8]) -> Result<Vec<u8>> {
    let mut channels = selected_channels.to_vec();
    channels.extend(args.passive_channels.iter().copied());
    channels.sort_unstable();
    channels.dedup();

    for &channel in &channels {
        ble_protocol::frequency_hz_from_channel_index(channel)?;
    }

    Ok(channels)
}

fn build_multi_channel_flowgraph(
    mut fg: &mut Flowgraph,
    args: &Args,
    sample_rate: f64,
    decode_channels: &[u8],
    capture_channels: &[u8],
    burst: bool,
) -> Result<()> {
    let pfb_channels = pfb_channels_from_sample_rate(sample_rate)?;
    let center_frequency = center_frequency_for_channels(capture_channels)?;
    let channel_rate = sample_rate / pfb_channels as f64;
    let samples_per_symbol = (channel_rate / 1.0e6).round() as usize;

    if (channel_rate - BLE_CHANNEL_SPACING_HZ).abs() > 1.0 {
        bail!(
            "multi-channel output rate must be 2 MS/s per channel, got {:.3} MS/s",
            channel_rate / 1.0e6
        );
    }

    println!(
        "Multi-channel mode: center={:.3} MHz sample_rate={:.3} MS/s pfb_channels={} channel_rate={:.3} MS/s decode_channels={:?} capture_channels={:?}",
        center_frequency / 1.0e6,
        sample_rate / 1.0e6,
        pfb_channels,
        channel_rate / 1.0e6,
        decode_channels,
        capture_channels,
    );

    let src = Builder::new(args.args.clone())?
        .frequency(center_frequency)
        .sample_rate(sample_rate)
        .gain(args.gain)
        .antenna(args.antenna.clone())
        .build_source()?;

    let taps = pfb_channelizer_taps(pfb_channels);
    let channelizer: PfbChannelizer = PfbChannelizer::new(pfb_channels, &taps, 1.0);
    connect!(fg, src.outputs[0] > channelizer);

    for &channel in &args.passive_channels {
        let frequency = ble_protocol::frequency_hz_from_channel_index(channel)?;
        let port = pfb_port_for_frequency(frequency, center_frequency, pfb_channels)?;
        println!(
            "capturing passive BLE channel {channel} ({:.3} MHz) on PFB output outputs[{port}]",
            frequency / 1.0e6
        );
    }

    let mut channel_ports = Vec::with_capacity(decode_channels.len());
    for &channel in decode_channels {
        let channel_frequency = ble_protocol::frequency_hz_from_channel_index(channel)?;
        let port = pfb_port_for_frequency(channel_frequency, center_frequency, pfb_channels)?;
        channel_ports.push((channel, channel_frequency, port));
    }
    let selected_ports: Vec<usize> = channel_ports.iter().map(|(_, _, port)| *port).collect();

    for port in 0..pfb_channels {
        if !selected_ports.contains(&port) {
            let snk = fg.add(NullSink::<Complex32>::new());
            fg.stream_dyn(channelizer, format!("outputs[{port}]"), snk, "input")?;
        }
    }

    for (channel, channel_frequency, port) in channel_ports {
        let port_name = format!("outputs[{port}]");
        println!(
            "connecting BLE channel {channel} ({:.3} MHz) to PFB output {port_name}",
            channel_frequency / 1.0e6
        );

        let gmsk_filter = fg.add(FirBuilder::fir::<Complex32, Complex32, _>(
            gmsk_demod::gmsk_rx_taps(samples_per_symbol)?,
        ));
        let snk = fg.add(NullSink::<u8>::new());

        fg.stream_dyn(channelizer, port_name, gmsk_filter, "input")?;

        if burst {
            let ble_burst = fg.add(ble_iq_burst::BleIqBurstBlock::<
                DefaultCpuReader<Complex32>,
                DefaultCpuWriter<u8>,
            >::with_packet_output(
                samples_per_symbol, channel, args.squelch_db, !args.quiet
            ));
            connect!(fg, gmsk_filter > ble_burst > snk);
        } else {
            let ble_sync = fg.add(ble_sync::BleSyncBlock::<
                DefaultCpuReader<f32>,
                DefaultCpuWriter<u8>,
            >::with_channel_phase_and_packet_output(
                args.threshold,
                args.normalize_levels,
                samples_per_symbol,
                channel,
                0,
                !args.quiet,
            ));
            let discriminator = fg.add(discriminator_block());
            connect!(fg, gmsk_filter > discriminator > ble_sync > snk);
        }
    }

    Ok(())
}

fn pfb_channelizer_taps(pfb_channels: usize) -> Vec<f32> {
    let transition_bw = 0.5;
    remez::low_pass(
        1.0,
        pfb_channels,
        0.5 - transition_bw / 2.0,
        0.5 + transition_bw / 2.0,
        0.1,
        100.0,
        None,
    )
    .into_iter()
    .map(|tap| tap as f32)
    .collect()
}

fn discriminator_block() -> Apply<impl FnMut(&Complex32) -> f32 + Send + 'static, Complex32, f32> {
    let mut last = Complex32::new(1.0, 0.0);
    Apply::new(move |v: &Complex32| {
        let mut phase = (v * last.conj()).arg();
        while phase <= -std::f32::consts::PI {
            phase += 2.0 * std::f32::consts::PI;
        }
        while phase > std::f32::consts::PI {
            phase -= 2.0 * std::f32::consts::PI;
        }
        last = *v;
        phase
    })
}

fn pfb_channels_from_sample_rate(sample_rate: f64) -> Result<usize> {
    let channels = sample_rate / BLE_CHANNEL_SPACING_HZ;
    let rounded = channels.round();

    if (channels - rounded).abs() > 1e-6 {
        bail!(
            "multi-channel sample rate must be an integer multiple of 2 MHz, got {:.3} MS/s",
            sample_rate / 1.0e6
        );
    }

    let channels = rounded as usize;
    if channels <= 2 {
        bail!("multi-channel mode needs at least 4 MHz sample rate");
    }

    Ok(channels)
}

fn center_frequency_for_channels(channels: &[u8]) -> Result<f64> {
    let frequencies_mhz: Vec<i32> = channels
        .iter()
        .map(|&channel| {
            ble_protocol::frequency_hz_from_channel_index(channel)
                .map(|freq| (freq / BLE_CHANNEL_SPACING_HZ).round() as i32)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let min_bin = frequencies_mhz.iter().min().copied().unwrap_or(1201);
    let max_bin = frequencies_mhz.iter().max().copied().unwrap_or(min_bin);
    let center_bin = min_bin + (max_bin - min_bin + 1) / 2;

    Ok(center_bin as f64 * BLE_CHANNEL_SPACING_HZ)
}

fn auto_multi_channel_sample_rate(channels: &[u8], center_frequency: f64) -> Result<f64> {
    let max_offset_bins = channels
        .iter()
        .map(|&channel| {
            ble_protocol::frequency_hz_from_channel_index(channel).map(|freq| {
                ((freq - center_frequency) / BLE_CHANNEL_SPACING_HZ)
                    .round()
                    .abs() as usize
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let pfb_channels = ((max_offset_bins + MULTI_CHANNEL_GUARD_BINS) * 2).max(4);
    Ok(pfb_channels as f64 * BLE_CHANNEL_SPACING_HZ)
}

fn pfb_port_for_frequency(
    frequency: f64,
    center_frequency: f64,
    pfb_channels: usize,
) -> Result<usize> {
    let offset_bins = (frequency - center_frequency) / BLE_CHANNEL_SPACING_HZ;
    let rounded = offset_bins.round();

    if (offset_bins - rounded).abs() > 1e-6 {
        bail!(
            "frequency {:.3} MHz does not align with the 2 MHz PFB grid around center {:.3} MHz",
            frequency / 1.0e6,
            center_frequency / 1.0e6,
        );
    }

    let offset_bins = rounded as isize;
    let half = (pfb_channels / 2) as isize;
    if offset_bins.abs() >= half {
        bail!(
            "frequency {:.3} MHz is outside the usable PFB span for center {:.3} MHz and {} channels",
            frequency / 1.0e6,
            center_frequency / 1.0e6,
            pfb_channels,
        );
    }

    if offset_bins >= 0 {
        Ok(offset_bins as usize)
    } else {
        Ok(pfb_channels - (-offset_bins as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::blocks::VectorSink;

    #[test]
    fn multi_channel_center_keeps_ble_grid_alignment() {
        let center = center_frequency_for_channels(&[37, 38]).unwrap();

        assert_eq!(center, 2.414e9);
    }

    #[test]
    fn pfb_ports_map_positive_and_negative_offsets() {
        let center = 2.414e9;

        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(37).unwrap(),
                center,
                16
            )
            .unwrap(),
            10
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(38).unwrap(),
                center,
                16
            )
            .unwrap(),
            6
        );
    }

    #[test]
    fn auto_sample_rate_leaves_guard_bins() {
        let center = center_frequency_for_channels(&[37, 38]).unwrap();

        assert_eq!(
            auto_multi_channel_sample_rate(&[37, 38], center).unwrap(),
            28.0e6
        );
    }

    #[test]
    fn adjacent_advertising_and_data_channel_fit_in_lower_rate() {
        let center = center_frequency_for_channels(&[37, 0]).unwrap();

        assert_eq!(center, 2.404e9);
        assert_eq!(
            auto_multi_channel_sample_rate(&[37, 0], center).unwrap(),
            8.0e6
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(37).unwrap(),
                center,
                4
            )
            .unwrap(),
            3
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(0).unwrap(),
                center,
                4
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn pfb_routes_synthetic_ble_channel_tones() -> Result<()> {
        let pfb_channels = 14;
        let sample_count = 16_384;
        let taps = pfb_channelizer_taps(pfb_channels);
        let center = center_frequency_for_channels(&[37, 38])?;

        let ch37_port = pfb_port_for_frequency(
            ble_protocol::frequency_hz_from_channel_index(37)?,
            center,
            pfb_channels,
        )?;
        let ch38_port = pfb_port_for_frequency(
            ble_protocol::frequency_hz_from_channel_index(38)?,
            center,
            pfb_channels,
        )?;

        let ch37_powers = pfb_output_powers_db(
            tone_at_pfb_bin(-6, pfb_channels, sample_count),
            pfb_channels,
            &taps,
        )?;
        let ch38_powers = pfb_output_powers_db(
            tone_at_pfb_bin(6, pfb_channels, sample_count),
            pfb_channels,
            &taps,
        )?;

        assert_eq!(strongest_output(&ch37_powers), ch37_port);
        assert_eq!(strongest_output(&ch38_powers), ch38_port);

        assert!(
            neighbor_leakage_db(&ch37_powers, ch37_port) < -30.0,
            "ch37 neighbor leakage {:.1} dB, powers {:?}",
            neighbor_leakage_db(&ch37_powers, ch37_port),
            ch37_powers
        );
        assert!(
            neighbor_leakage_db(&ch38_powers, ch38_port) < -30.0,
            "ch38 neighbor leakage {:.1} dB, powers {:?}",
            neighbor_leakage_db(&ch38_powers, ch38_port),
            ch38_powers
        );

        Ok(())
    }

    fn tone_at_pfb_bin(bin: isize, pfb_channels: usize, sample_count: usize) -> Vec<Complex32> {
        let phase_step = 2.0 * std::f64::consts::PI * bin as f64 / pfb_channels as f64;
        (0..sample_count)
            .map(|index| {
                let phase = phase_step * index as f64;
                Complex32::new(phase.cos() as f32, phase.sin() as f32)
            })
            .collect()
    }

    fn pfb_output_powers_db(
        input: Vec<Complex32>,
        pfb_channels: usize,
        taps: &[f32],
    ) -> Result<Vec<f64>> {
        let mut fg = Flowgraph::new();
        let src = fg.add(VectorSource::<Complex32>::new(input));
        let channelizer = fg.add(PfbChannelizer::<
            DefaultCpuReader<Complex32>,
            DefaultCpuWriter<Complex32>,
        >::new(pfb_channels, taps, 1.0));
        fg.stream_dyn(src, "output", channelizer, "input")?;

        let mut sinks = Vec::with_capacity(pfb_channels);
        for port in 0..pfb_channels {
            let sink = fg.add(VectorSink::<Complex32>::new(2048));
            fg.stream_dyn(channelizer, format!("outputs[{port}]"), sink, "input")?;
            sinks.push(sink);
        }

        let fg = Runtime::new().run(fg)?;
        let powers = sinks
            .iter()
            .map(|sink| {
                let sink = fg.block(sink)?;
                Ok(mean_power_db(sink.items()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(powers)
    }

    fn mean_power_db(samples: &[Complex32]) -> f64 {
        let skip = samples.len().min(16);
        let samples = &samples[skip..];
        let mean_power = samples
            .iter()
            .map(|sample| sample.norm_sqr() as f64)
            .sum::<f64>()
            / samples.len().max(1) as f64;

        10.0 * mean_power.max(1.0e-20).log10()
    }

    fn strongest_output(powers_db: &[f64]) -> usize {
        powers_db
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap()
    }

    fn neighbor_leakage_db(powers_db: &[f64], main_port: usize) -> f64 {
        let main_power = powers_db[main_port];
        let lower = powers_db[(main_port + powers_db.len() - 1) % powers_db.len()] - main_power;
        let upper = powers_db[(main_port + 1) % powers_db.len()] - main_power;
        lower.max(upper)
    }
}
