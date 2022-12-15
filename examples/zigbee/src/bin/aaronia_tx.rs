use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::blocks::aaronia;
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
    /// Url
    #[clap(short, long, default_value = "http://127.0.0.1:54664")]
    url: String,
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

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let mac = fg.add_block(Mac::new());
    let modulator = fg.add_block(modulator());
    let iq_delay = fg.add_block(IqDelay::new());
    let snk = fg.add_block(aaronia::HttpSink::new(
        rt.scheduler(),
        &args.url,
        args.freq,
        args.sample_rate,
    ));

    fg.connect_stream(mac, "out", modulator, "in")?;
    fg.connect_stream(modulator, "out", iq_delay, "in")?;
    fg.connect_stream(iq_delay, "out", snk, "in")?;

    let (fg, mut handle) = block_on(rt.start(fg));
    println!("started");

    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.5)).await;
            println!("sending frame");
            handle
                .call(
                    0,
                    1,
                    // Pmt::Blob(format!("FutureSDR {seq}").as_bytes().to_vec()),
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
