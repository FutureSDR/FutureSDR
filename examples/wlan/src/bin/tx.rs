use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::{block_on, Timer};
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamInput;
use futuresdr::runtime::StreamOutput;

use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(long, default_value_t = 26)]
    rx_channel: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {:?}", args);

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
    let prefix = fg.add_block(Prefix::new(10000, 10000));
    fg.connect_stream(fft, "out", prefix, "in")?;
    let soapy_snk = fg.add_block(SoapySinkBuilder::new().freq(5.24e9).sample_rate(20e6).gain(60.0).build());
    fg.connect_stream(prefix, "out", soapy_snk, "in")?;

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

fn fft_tag_propagation(inputs: &mut [StreamInput], outputs: &mut [StreamOutput]) {
    debug_assert_eq!(inputs[0].consumed().0, outputs[0].produced());
    let (n, tags) = inputs[0].consumed();
    // println!("fft produced {}   consumed {}   tags {:?}", outputs[0].produced(), n, tags);
    for t in tags.iter().filter(|x| x.index < n) {
        outputs[0].add_tag_abs(t.index, t.tag.clone());
    }
}
