use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::{block_on, Timer};
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use wlan::fft_tag_propagation;
use wlan::parse_channel;
use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;

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
    #[clap(short, long, default_value_t = 20e6)]
    sample_rate: f64,
    /// WLAN Channel Number
    #[clap(short, long, value_parser = parse_channel, default_value = "34")]
    channel: f64,
}

use wlan::MAX_SYM;
const PAD_FRONT: usize = 5000;
const PAD_TAIL: usize = 5000;

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {:?}", args);

    let mut size = 4096;
    let prefix_in_size = loop {
        if size / 8 >= MAX_SYM * 64 {
            break size;
        }
        size += 4096
    };
    let mut size = 4096;
    let prefix_out_size = loop {
        if size / 8 >= PAD_FRONT + std::cmp::max(PAD_TAIL, 1) + 320 + MAX_SYM * 80 {
            break size;
        }
        size += 4096
    };

    let mut fg = Flowgraph::new();
    let mac = fg.add_block(Mac::new([0x42; 6], [0x23; 6], [0xff; 6]));
    let encoder = fg.add_block(Encoder::new(Mcs::Qpsk_1_2));
    fg.connect_message(mac, "tx", encoder, "tx")?;
    let mapper = fg.add_block(Mapper::new());
    fg.connect_stream(encoder, "out", mapper, "in")?;
    let mut fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt() * 0.6),
    );
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let fft = fg.add_block(fft);
    fg.connect_stream(mapper, "out", fft, "in")?;
    let prefix = fg.add_block(Prefix::new(PAD_FRONT, PAD_TAIL));
    fg.connect_stream_with_type(
        fft,
        "out",
        prefix,
        "in",
        Circular::with_size(prefix_in_size),
    )?;
    let mut soapy = SoapySinkBuilder::new()
        .freq(args.channel)
        .sample_rate(args.sample_rate)
        .gain(args.gain);
    if let Some(a) = args.antenna {
        soapy = soapy.antenna(a);
    }
    if let Some(f) = args.filter {
        soapy = soapy.filter(f);
    }

    let soapy_snk = fg.add_block(soapy.build());
    fg.connect_stream_with_type(
        prefix,
        "out",
        soapy_snk,
        "in",
        Circular::with_size(prefix_out_size),
    )?;

    let rt = Runtime::new();
    let (_fg, mut handle) = block_on(rt.start(fg));

    let mut seq = 0u64;
    rt.block_on(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    0,
                    0,
                    Pmt::Any(Box::new((
                        format!("FutureSDR {}", seq).as_bytes().to_vec(),
                        Mcs::Qpsk_1_2,
                    ))),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    Ok(())
}
