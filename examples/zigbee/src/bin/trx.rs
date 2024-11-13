use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::blocks::seify::SinkBuilder;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use zigbee::modulator;
use zigbee::parse_channel;
use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::IqDelay;
use zigbee::Mac;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(id = "rx-channel", long, value_parser = parse_channel, default_value = "26")]
    rx_freq: f64,
    #[clap(id = "tx-channel", long, value_parser = parse_channel, default_value = "26")]
    tx_freq: f64,
    #[clap(long, default_value_t = 50.0)]
    rx_gain: f64,
    #[clap(long, default_value_t = 18.0)]
    tx_gain: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    // ========================================
    // TRANSMITTER
    // ========================================
    let mac = fg.add_block(Mac::new())?;
    let modulator = fg.add_block(modulator())?;
    let iq_delay = fg.add_block(IqDelay::new())?;
    let snk = fg.add_block(
        SinkBuilder::new()
            .frequency(args.tx_freq)
            .sample_rate(4e6)
            .gain(args.tx_gain)
            .build()?,
    )?;

    fg.connect_stream(mac, "out", modulator, "in")?;
    fg.connect_stream(modulator, "out", iq_delay, "in")?;
    fg.connect_stream(iq_delay, "out", snk, "in")?;

    // ========================================
    // Receiver
    // ========================================
    let src = fg.add_block(
        SourceBuilder::new()
            .frequency(args.rx_freq)
            .sample_rate(4e6)
            .gain(args.rx_gain)
            .build()?,
    )?;

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = fg.add_block(Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    }))?;

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = fg.add_block(ClockRecoveryMm::new(
        omega,
        gain_omega,
        mu,
        gain_mu,
        omega_relative_limit,
    ))?;

    let decoder = fg.add_block(Decoder::new(6))?;

    fg.connect_stream(src, "out", avg, "in")?;
    fg.connect_stream(avg, "out", mm, "in")?;
    fg.connect_stream(mm, "out", decoder, "in")?;
    fg.connect_message(decoder, "out", mac, "rx")?;

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start_sync(fg);

    // send a message every 0.8 seconds
    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    0, // mac block
                    1, // tx handler
                    Pmt::Blob(format!("FutureSDR {seq}").as_bytes().to_vec()),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    block_on(fg)?;

    Ok(())
}
