use clap::{Arg, Command};
use std::time::Duration;

use futuresdr::anyhow::{Context, Result};
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::TagDebug;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use zigbee::channel_to_freq;
use zigbee::modulator;
use zigbee::IqDelay;
use zigbee::Mac;

fn main() -> Result<()> {
    let matches = Command::new("ZigBee Transmitter")
        .arg(
            Arg::new("channel")
                .short('c')
                .long("channel")
                .takes_value(true)
                .value_name("CHANNEL")
                .default_value("26")
                .help("Channel (11..=26)."),
        )
        .get_matches();

    let channel: u32 = matches.value_of_t("channel").context("invalid channel")?;
    let _freq = channel_to_freq(channel)?;

    let mut fg = Flowgraph::new();

    let mac = fg.add_block(Mac::new());
    let modulator = fg.add_block(modulator());
    let file_snk = fg.add_block(FileSink::<Complex32>::new("/tmp/foo.cf32"));
    let iq_delay = fg.add_block(IqDelay::new());
    let snk = fg.add_block(NullSink::<Complex32>::new());
    let tag_debug = fg.add_block(TagDebug::<u8>::new("mac"));
    let tag_debug_mod = fg.add_block(TagDebug::<Complex32>::new("mod"));

    fg.connect_stream(mac, "out", modulator, "in")?;
    fg.connect_stream(mac, "out", tag_debug, "in")?;
    fg.connect_stream(modulator, "out", snk, "in")?;
    fg.connect_stream(modulator, "out", iq_delay, "in")?;
    fg.connect_stream(iq_delay, "out", file_snk, "in")?;
    fg.connect_stream(modulator, "out", tag_debug_mod, "in")?;

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start(fg);

    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    0,
                    1,
                    Pmt::Blob(format!("FutureSDR {}", seq).as_bytes().to_vec()),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    block_on(fg)?;

    Ok(())
}
