use anyhow::Result;
use clap::Parser;
use futuresdr::runtime::Timer;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use std::time::Duration;

use zigbee::IqDelay;
use zigbee::Mac;
use zigbee::modulator;
use zigbee::parse_channel;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Args
    #[clap(short, long)]
    args: Option<String>,
    /// Gain
    #[clap(short, long, default_value_t = 60.0)]
    gain: f64,
    /// Sample Rate
    #[clap(short, long, default_value_t = 4e6)]
    sample_rate: f64,
    /// Zigbee Channel Number (11..26)
    #[clap(id = "channel", short, long, value_parser = parse_channel, default_value = "26")]
    freq: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    let mac: Mac = Mac::new();
    let mac = fg.add(mac);
    let modulator = modulator(&mut fg);
    let iq_delay: IqDelay = IqDelay::new();
    let iq_delay = fg.add(iq_delay);

    let snk = Builder::new(args.args)?
        .frequency(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .antenna(args.antenna)
        .min_in_buffer_size(98304)
        .build_sink()?;
    let snk = fg.add(snk);

    fg.stream_dyn(mac, "output", modulator, "input")?;
    fg.stream_dyn(modulator, "output", iq_delay, "input")?;
    fg.stream_dyn(iq_delay, "output", snk, "inputs[0]")?;
    let mac = mac.id();

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let handle = running.handle();

    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .post(
                    mac,
                    "tx",
                    Pmt::Blob(format!("FutureSDR {seq}").as_bytes().to_vec()),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    Runtime::block_on(running.wait())?;

    Ok(())
}
