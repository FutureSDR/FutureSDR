use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use zigbee::modulator;
use zigbee::parse_channel;
use zigbee::IqDelay;
use zigbee::Mac;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Antenna
    #[clap(short, long)]
    antenna: Option<String>,
    /// Soapy Filter
    #[clap(short, long)]
    filter: Option<String>,
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

    let mac = fg.add_block(Mac::new());
    let modulator = fg.add_block(modulator());
    let iq_delay = fg.add_block(IqDelay::new());

    let mut soapy = SoapySinkBuilder::new()
        .freq(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain);
    if let Some(a) = args.antenna {
        soapy = soapy.antenna(a);
    }
    if let Some(f) = args.filter {
        soapy = soapy.filter(f);
    }

    let soapy_snk = fg.add_block(soapy.build());

    fg.connect_stream(mac, "out", modulator, "in")?;
    fg.connect_stream(modulator, "out", iq_delay, "in")?;
    fg.connect_stream(iq_delay, "out", soapy_snk, "in")?;

    let rt = Runtime::new();
    let (fg, mut handle) = block_on(rt.start(fg));

    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    0,
                    1,
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
