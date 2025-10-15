use clap::Parser;
use futuresdr::async_io::Timer;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use std::time::Duration;

use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;
use wlan::parse_channel;

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
    #[clap(short, long, default_value_t = 20e6)]
    sample_rate: f64,
    /// WLAN Channel Number
    #[clap(short, long, value_parser = parse_channel, default_value = "34")]
    channel: f64,
}

const PAD_FRONT: usize = 5000;
const PAD_TAIL: usize = 5000;

fn main() -> Result<()> {
    let args = Args::parse();
    futuresdr::runtime::init();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();
    let mac = Mac::new([0x42; 6], [0x23; 6], [0xff; 6]);
    let encoder: Encoder = Encoder::new(Mcs::Qpsk_1_2);
    connect!(fg, mac.tx | tx.encoder);
    let mapper: Mapper = Mapper::new();
    connect!(fg, encoder > mapper);
    let fft: Fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt()),
    );
    connect!(fg, mapper > fft);
    let prefix: Prefix = Prefix::new(PAD_FRONT, PAD_TAIL);
    connect!(fg, fft > prefix);
    let snk = Builder::new(args.args)?
        .frequency(args.channel)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_sink()?;

    connect!(fg, prefix > inputs[0].snk);

    let mac = mac.get()?.id;

    let rt = Runtime::new();
    let (_fg, mut handle) = rt.start_sync(fg)?;

    let mut seq = 0u64;
    rt.block_on(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.1)).await;
            handle
                .call(
                    mac,
                    "tx",
                    Pmt::Any(Box::new((
                        format!("FutureSDR {seq}xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").as_bytes().to_vec(),
                        Mcs::Qam16_1_2,
                    ))),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    Ok(())
}
