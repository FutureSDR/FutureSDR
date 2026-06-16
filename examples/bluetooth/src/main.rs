use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::seify::Builder;
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::runtime::Error as RuntimeError;
use futuresdr::runtime::dev::DefaultCpuReader;
use futuresdr::runtime::dev::DefaultCpuWriter;
use std::sync::mpsc;
use std::time::Duration;

mod ble_ad;
mod ble_detector;
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
    let samples_per_symbol = (args.sample_rate / 1e6).round().max(1.0) as usize;
    let symbol_rate_ratio = args.sample_rate / 1e6;
    if (symbol_rate_ratio - samples_per_symbol as f64).abs() > 0.05 {
        println!(
            "Sample rate {:.3} MS/s is not close to an integer multiple of the BLE 1 Msym/s symbol rate; symbol slicing may be less reliable",
            args.sample_rate / 1.0e6
        );
    }
    let snk = NullSink::<u8>::new();
    let channel_index = args.channel;
    let frequency = ble_protocol::frequency_hz_from_channel_index(channel_index)?;
    println!(
        "Using BLE channel {channel_index} ({:.3} MHz)",
        frequency / 1.0e6
    );

    let rx_taps = gmsk_demod::gmsk_rx_taps(samples_per_symbol)?;
    println!("GMSK receive filter configured.");
    let initial_delay = 0;

    let ble_sync = ble_sync::BleSyncBlock::<DefaultCpuReader<f32>, DefaultCpuWriter<u8>>::with_channel_and_phase(
        args.threshold,
        args.normalize_levels,
        samples_per_symbol,
        channel_index,
        initial_delay,
    );

    let gmsk_filter = FirBuilder::fir::<Complex32, Complex32, _>(rx_taps);

    let mut last = Complex32::new(1.0, 0.0);
    let discriminator = Apply::new(move |v: &Complex32| {
        let mut phase = (v * last.conj()).arg();
        while phase <= -std::f32::consts::PI {
            phase += 2.0 * std::f32::consts::PI;
        }
        while phase > std::f32::consts::PI {
            phase -= 2.0 * std::f32::consts::PI;
        }
        last = *v;
        phase
    });

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
        let src = Builder::new(args.args)?
            .frequency(frequency)
            .sample_rate(args.sample_rate)
            .gain(args.gain)
            .antenna(args.antenna)
            .build_source()?;

        connect!(fg, src.outputs[0] > gmsk_filter > discriminator > ble_sync > snk);
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
