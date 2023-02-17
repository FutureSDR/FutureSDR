use clap::Parser;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::StreamExt;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessagePipe;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use wlan::fft_tag_propagation;
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
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Args
    #[clap(short, long)]
    args: Option<String>,
    /// Gain
    #[clap(short, long, default_value_t = 28.0)]
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
    println!("Configuration: {args:?}");

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let mut seify = SourceBuilder::with_scheduler(rt.scheduler())
        .frequency(args.channel)
        .sample_rate(args.sample_rate)
        .gain(args.gain);
    if let Some(ref s) = args.args {
        seify = seify.args(s)?;
    }
    if let Some(ref s) = args.antenna {
        seify = seify.antenna(s);
    }

    let src = seify.build()?;
    let delay = Delay::<Complex32>::new(16);
    connect!(fg, src > delay);

    let complex_to_mag_2 = Apply::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    connect!(fg, src > complex_to_mag_2 > float_avg);

    let mult_conj = Combine::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    connect!(fg, src > in0.mult_conj.out > complex_avg;
                 delay > mult_conj.in1);

    let divide_mag = Combine::new(|a: &Complex32, b: &f32| a.norm() / b);
    connect!(fg, complex_avg > divide_mag.in0; float_avg > divide_mag.in1);

    let sync_short = SyncShort::new();
    connect!(fg, delay > sync_short.in_sig;
                 complex_avg > sync_short.in_abs;
                 divide_mag > sync_short.in_cor);

    let sync_long = SyncLong::new();
    connect!(fg, sync_short > sync_long);

    let mut fft = Fft::new(64);
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let frame_equalizer = FrameEqualizer::new();
    let decoder = Decoder::new();
    connect!(fg, sync_long > fft > frame_equalizer > decoder);

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    let udp1 = futuresdr::blocks::BlobToUdp::new("127.0.0.1:55555");
    let udp2 = futuresdr::blocks::BlobToUdp::new("127.0.0.1:55556");
    connect!(fg, decoder.rx_frames | message_pipe;
                 decoder.rx_frames | udp1;
                 decoder.rftap | udp2);

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
