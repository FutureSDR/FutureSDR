use anyhow::Result;
use clap::Parser;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use std::time::Duration;

use zigbee::modulator;
use zigbee::parse_channel;
use zigbee::IqDelay;
use zigbee::Mac;

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
    let mac = fg.add_block(mac);
    let modulator = modulator(&mut fg);
    let iq_delay: IqDelay = IqDelay::new();
    let iq_delay = fg.add_block(iq_delay);

    let snk = Builder::new(args.args)?
        .frequency(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_sink()?;
    let snk = fg.add_block(snk);

    fg.connect_dyn(&mac, "output", modulator, "input")?;
    fg.connect_dyn(modulator, "output", &iq_delay, "input")?;
    fg.connect_dyn(iq_delay, "output", snk, "inputs[0]")?;
    let mac = mac.into();

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start_sync(fg);

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
