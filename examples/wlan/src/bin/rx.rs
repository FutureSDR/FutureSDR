use clap::Parser;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::StreamExt;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamInput;
use futuresdr::runtime::StreamOutput;

use wlan::parse_channel;
use wlan::Decoder;
use wlan::Delay;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

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

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {:?}", args);

    let mut fg = Flowgraph::new();

    let mut soapy = SoapySourceBuilder::new()
        .freq(args.channel)
        .sample_rate(args.sample_rate)
        .gain(args.gain);
    if let Some(a) = args.antenna {
        soapy = soapy.antenna(a);
    }
    if let Some(f) = args.filter {
        soapy = soapy.filter(f);
    }
    let src = fg.add_block(soapy.build());
    let delay = fg.add_block(Delay::<Complex32>::new(16));
    fg.connect_stream(src, "out", delay, "in")?;

    let complex_to_mag_2 = fg.add_block(Apply::new(|i: &Complex32| i.norm_sqr()));
    let float_avg = fg.add_block(MovingAverage::<f32>::new(64));
    fg.connect_stream(src, "out", complex_to_mag_2, "in")?;
    fg.connect_stream(complex_to_mag_2, "out", float_avg, "in")?;

    let mult_conj = fg.add_block(Combine::new(|a: &Complex32, b: &Complex32| a * b.conj()));
    let complex_avg = fg.add_block(MovingAverage::<Complex32>::new(48));
    fg.connect_stream(src, "out", mult_conj, "in0")?;
    fg.connect_stream(delay, "out", mult_conj, "in1")?;
    fg.connect_stream(mult_conj, "out", complex_avg, "in")?;

    let divide_mag = fg.add_block(Combine::new(|a: &Complex32, b: &f32| a.norm() / b));
    fg.connect_stream(complex_avg, "out", divide_mag, "in0")?;
    fg.connect_stream(float_avg, "out", divide_mag, "in1")?;

    let sync_short = fg.add_block(SyncShort::new());
    fg.connect_stream(delay, "out", sync_short, "in_sig")?;
    fg.connect_stream(complex_avg, "out", sync_short, "in_abs")?;
    fg.connect_stream(divide_mag, "out", sync_short, "in_cor")?;

    let sync_long = fg.add_block(SyncLong::new());
    fg.connect_stream(sync_short, "out", sync_long, "in")?;

    let mut fft = Fft::new(64);
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let fft = fg.add_block(fft);
    fg.connect_stream(sync_long, "out", fft, "in")?;

    let frame_equalizer = fg.add_block(FrameEqualizer::new());
    fg.connect_stream(fft, "out", frame_equalizer, "in")?;

    let decoder = fg.add_block(Decoder::new());
    fg.connect_stream(frame_equalizer, "out", decoder, "in")?;

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = fg.add_block(MessagePipe::new(tx_frame));
    fg.connect_message(decoder, "rx_frames", message_pipe, "in")?;
    let blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55555"));
    fg.connect_message(decoder, "rx_frames", blob_to_udp, "in")?;
    let blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55556"));
    fg.connect_message(decoder, "rftap", blob_to_udp, "in")?;

    let rt = Runtime::new();
    let (_fg, _handle) = block_on(rt.start(fg));
    rt.block_on(async move {
        while let Some(x) = rx_frame.next().await {
            match x {
                Pmt::Blob(data) => {
                    println!("received frame ({:?} bytes)", data.len());
                }
                _ => break,
            }
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
