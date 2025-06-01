use anyhow::Result;
use clap::Parser;
use futuresdr::async_io::Timer;
use futuresdr::async_io::block_on;
use futuresdr::blocks::Apply;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use std::time::Duration;

use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::IqDelay;
use zigbee::Mac;
use zigbee::modulator;
use zigbee::parse_channel;

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
    let mac: Mac = Mac::new();
    let mac = fg.add_block(mac);
    let modulator = modulator(&mut fg);
    let iq_delay: IqDelay = IqDelay::new();
    let iq_delay = fg.add_block(iq_delay);
    let snk = fg.add_block(
        Builder::new("")?
            .frequency(args.tx_freq)
            .sample_rate(4e6)
            .gain(args.tx_gain)
            .build_sink()?,
    );

    fg.connect_dyn(&mac, "output", modulator, "input")?;
    fg.connect_dyn(modulator, "output", &iq_delay, "input")?;
    fg.connect_dyn(iq_delay, "output", snk, "inputs[0]")?;

    // ========================================
    // Receiver
    // ========================================
    let src = Builder::new("")?
        .frequency(args.rx_freq)
        .sample_rate(4e6)
        .gain(args.rx_gain)
        .build_source()?;

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = Apply::<_, _, _>::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    });

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm: ClockRecoveryMm =
        ClockRecoveryMm::new(omega, gain_omega, mu, gain_mu, omega_relative_limit);

    let decoder = Decoder::new(6);

    connect!(fg, src.outputs[0] > avg > mm > decoder);
    connect!(fg, decoder | rx.mac);
    let mac = mac.into();

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start_sync(fg);

    // send a message every 0.8 seconds
    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    mac,
                    "tx",
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
