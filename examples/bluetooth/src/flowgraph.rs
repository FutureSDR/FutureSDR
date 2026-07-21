use anyhow::Result;
use anyhow::bail;
use futuresdr::blocks::Apply;
use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::PfbChannelizer;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::seify::Builder;
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::dev::DefaultCpuReader;
use futuresdr::runtime::dev::DefaultCpuWriter;

use crate::ble_connection::SharedConnectionTable;
use crate::ble_iq_burst;
use crate::ble_protocol;
use crate::ble_protocol::BlePhy;
use crate::ble_sim;
use crate::ble_sync;
use crate::channelizer::BLE_CHANNEL_SPACING_HZ;
use crate::channelizer::center_frequency_for_channels;
use crate::channelizer::pfb_channelizer_taps;
use crate::channelizer::pfb_channels_from_sample_rate;
use crate::channelizer::pfb_port_for_frequency;
use crate::cli::Args;
use crate::cli::DecoderMode;
use crate::gmsk_demod;

pub(crate) fn build_single_channel_flowgraph(
    mut fg: &mut Flowgraph,
    args: &Args,
    sample_rate: f64,
) -> Result<()> {
    let phy = BlePhy::Le1M;
    let channel_index = args.channels[0];
    let connections = SharedConnectionTable::with_monitored_channels(&[channel_index]);
    let symbol_rate = phy.symbol_rate_hz();
    let samples_per_symbol = (sample_rate / symbol_rate).round().max(1.0) as usize;
    let symbol_rate_ratio = sample_rate / symbol_rate;
    if (symbol_rate_ratio - samples_per_symbol as f64).abs() > 0.05 {
        println!(
            "Sample rate {:.3} MS/s is not close to an integer multiple of the BLE 1 Msym/s symbol rate; symbol slicing may be less reliable",
            sample_rate / 1.0e6
        );
    }

    let frequency = ble_protocol::frequency_hz_from_channel_index(channel_index)?;
    println!(
        "Using BLE channel {channel_index} ({:.3} MHz)",
        frequency / 1.0e6
    );
    if args.decoder_mode() == DecoderMode::Burst {
        match args.squelch_db {
            Some(threshold_db) => println!(
                "Using IQ burst decoder with fixed squelch threshold {threshold_db:.1} dB."
            ),
            None => println!(
                "Using IQ burst decoder with adaptive squelch margin {:.1} dB.",
                args.squelch_margin_db
            ),
        }
    } else {
        println!("Using continuous packet decoder.");
    }

    let rx_taps = gmsk_demod::gmsk_rx_taps(samples_per_symbol)?;
    println!("GMSK receive filter configured.");
    let snk = NullSink::<u8>::new();

    if args.decoder_mode() == DecoderMode::Burst {
        let ble_burst = ble_iq_burst::BleIqBurstBlock::<
            DefaultCpuReader<Complex32>,
            DefaultCpuWriter<u8>,
        >::with_packet_and_wireshark_output(
            phy,
            samples_per_symbol,
            channel_index,
            rx_taps,
            burst_squelch(args),
            ble_iq_burst::BleIqBurstOptions {
                print_packets: !args.quiet,
                wireshark_output: args.wireshark,
                follow_connections: args.follow_connections,
            },
        )?
        .with_connections(connections.clone());

        if args.simulate {
            println!("Simulation mode: Testing full GMSK demodulator DSP chain");
            let simulated_samples = ble_sim::generate_ble_packet_samples(
                samples_per_symbol,
                args.simulate_packets,
                channel_index,
            );
            let src = VectorSource::<Complex32>::new(simulated_samples);
            connect!(fg, src > ble_burst > snk);
            connect_wireshark(fg, ble_burst, args)?;
        } else if let Some(iq_file) = &args.iq_file {
            println!(
                "File mode: Reading interleaved f32 IQ samples from {}",
                iq_file.display()
            );
            let src = FileSource::<Complex32>::new(iq_file, false);
            connect!(fg, src > ble_burst > snk);
            connect_wireshark(fg, ble_burst, args)?;
        } else {
            println!("Hardware mode: Deploying Seify source live SDR chain");
            let src = Builder::new(args.args.clone())?
                .frequency(frequency)
                .sample_rate(sample_rate)
                .gain(args.gain)
                .antenna(args.antenna.clone())
                .build_source()?;
            connect!(fg, src.outputs[0] > ble_burst > snk);
            fg.message(src, "overflow", ble_burst, "rx_overflow")?;
            connect_wireshark(fg, ble_burst, args)?;
        }
    } else {
        let gmsk_filter = FirBuilder::fir::<Complex32, Complex32, _>(rx_taps);
        let ble_sync = ble_sync::BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_phase_packet_and_wireshark_output(
            samples_per_symbol,
            channel_index,
            0,
            !args.quiet,
            args.wireshark,
        )
        .with_connections(connections);
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
            connect_wireshark(fg, ble_sync, args)?;
        } else if let Some(iq_file) = &args.iq_file {
            println!(
                "File mode: Reading interleaved f32 IQ samples from {}",
                iq_file.display()
            );
            let src = FileSource::<Complex32>::new(iq_file, false);
            connect!(fg, src > gmsk_filter > discriminator > ble_sync > snk);
            connect_wireshark(fg, ble_sync, args)?;
        } else {
            println!("Hardware mode: Deploying Seify source live SDR chain");
            let src = Builder::new(args.args.clone())?
                .frequency(frequency)
                .sample_rate(sample_rate)
                .gain(args.gain)
                .antenna(args.antenna.clone())
                .build_source()?;
            connect!(fg, src.outputs[0] > gmsk_filter > discriminator > ble_sync > snk);
            connect_wireshark(fg, ble_sync, args)?;
        }
    }

    Ok(())
}

pub(crate) fn build_multi_channel_flowgraph(
    mut fg: &mut Flowgraph,
    args: &Args,
    sample_rate: f64,
    decode_channels: &[u8],
) -> Result<()> {
    let pfb_channels = pfb_channels_from_sample_rate(sample_rate)?;
    let center_frequency = center_frequency_for_channels(decode_channels)?;
    let channel_rate = sample_rate / pfb_channels as f64;
    let phy = BlePhy::Le1M;
    let connections = SharedConnectionTable::with_monitored_channels(decode_channels);
    let samples_per_symbol = (channel_rate / phy.symbol_rate_hz()).round() as usize;

    if (channel_rate - BLE_CHANNEL_SPACING_HZ).abs() > 1.0 {
        bail!(
            "multi-channel output rate must be 2 MS/s per channel, got {:.3} MS/s",
            channel_rate / 1.0e6
        );
    }

    println!(
        "Multi-channel mode: center={:.3} MHz sample_rate={:.3} MS/s pfb_channels={} channel_rate={:.3} MS/s channels={:?}",
        center_frequency / 1.0e6,
        sample_rate / 1.0e6,
        pfb_channels,
        channel_rate / 1.0e6,
        decode_channels,
    );
    if args.follow_connections {
        println!(
            "Connection follower enabled: gating locked data contexts around predicted event windows."
        );
    }

    let src = Builder::new(args.args.clone())?
        .frequency(center_frequency)
        .sample_rate(sample_rate)
        .gain(args.gain)
        .antenna(args.antenna.clone())
        .build_source()?;

    let taps = pfb_channelizer_taps(pfb_channels);
    let channelizer: PfbChannelizer = PfbChannelizer::new(pfb_channels, &taps, 1.0);
    connect!(fg, src.outputs[0] > channelizer);

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

    let mut overflow_receiver_connected = false;
    for (channel, channel_frequency, port) in channel_ports {
        let port_name = format!("outputs[{port}]");
        println!(
            "connecting BLE channel {channel} ({:.3} MHz) to PFB output {port_name}",
            channel_frequency / 1.0e6
        );

        let rx_taps = gmsk_demod::gmsk_rx_taps(samples_per_symbol)?;
        let snk = fg.add(NullSink::<u8>::new());

        if args.decoder_mode() == DecoderMode::Burst {
            let ble_burst =
                fg.add(
                    ble_iq_burst::BleIqBurstBlock::<
                        DefaultCpuReader<Complex32>,
                        DefaultCpuWriter<u8>,
                    >::with_packet_and_wireshark_output(
                        phy,
                        samples_per_symbol,
                        channel,
                        rx_taps,
                        burst_squelch(args),
                        ble_iq_burst::BleIqBurstOptions {
                            print_packets: !args.quiet,
                            wireshark_output: args.wireshark,
                            follow_connections: args.follow_connections,
                        },
                    )?
                    .with_connections(connections.clone()),
                );
            fg.stream_dyn(channelizer, port_name, ble_burst, "input")?;
            connect!(fg, ble_burst > snk);
            if !overflow_receiver_connected {
                fg.message(src, "overflow", ble_burst, "rx_overflow")?;
                overflow_receiver_connected = true;
            }
            connect_wireshark(fg, ble_burst, args)?;
        } else {
            let gmsk_filter = fg.add(FirBuilder::fir::<Complex32, Complex32, _>(rx_taps));
            let ble_sync = fg.add(ble_sync::BleSyncBlock::<
                DefaultCpuReader<f32>,
                DefaultCpuWriter<u8>,
            >::with_channel_phase_packet_and_wireshark_output(
                samples_per_symbol,
                channel,
                0,
                !args.quiet,
                args.wireshark,
            )
            .with_connections(connections.clone()));
            let discriminator = fg.add(discriminator_block());
            fg.stream_dyn(channelizer, port_name, gmsk_filter, "input")?;
            connect!(fg, gmsk_filter > discriminator > ble_sync > snk);
            connect_wireshark(fg, ble_sync, args)?;
        }
    }

    Ok(())
}

fn burst_squelch(args: &Args) -> ble_iq_burst::SquelchConfig {
    match args.squelch_db {
        Some(threshold_db) => ble_iq_burst::SquelchConfig::fixed(threshold_db),
        None => ble_iq_burst::SquelchConfig::adaptive(args.squelch_margin_db),
    }
}

fn connect_wireshark(fg: &mut Flowgraph, source: impl Into<BlockId>, args: &Args) -> Result<()> {
    if !args.wireshark {
        return Ok(());
    }

    let udp = fg.add(BlobToUdp::new(&args.wireshark_addr));
    fg.message(source.into(), "wireshark", udp, "in")?;
    println!(
        "Streaming BLE LL packets to Wireshark UDP endpoint {}.",
        args.wireshark_addr
    );
    Ok(())
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
