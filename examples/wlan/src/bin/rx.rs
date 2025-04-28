use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::WebsocketPmtSink;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::futures::StreamExt;
use futuresdr::futures::channel::mpsc;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::BlockT;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Result;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::copy_tag_propagation;

use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;
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
    #[clap(short, long, default_value_t = 28.0)]
    gain: f64,
    /// Sample Rate
    #[clap(short, long, default_value_t = 20e6)]
    sample_rate: f64,
    /// WLAN Channel Number
    #[clap(short, long, value_parser = parse_channel, default_value = "34")]
    channel: f64,
    /// DC Offset
    #[clap(short, long, default_value_t = false)]
    dc_offset: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let src = SourceBuilder::new()
        .frequency(args.channel)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .antenna(args.antenna)
        .args(args.args)?
        .build()?;

    connect!(fg, src);

    let prev = if args.dc_offset {
        let mut avg_real = 0.0;
        let mut avg_img = 0.0;
        let ratio = 1.0e-5;
        let dc = Apply::new(move |c: &Complex32| -> Complex32 {
            avg_real = ratio * (c.re - avg_real) + avg_real;
            avg_img = ratio * (c.im - avg_img) + avg_img;
            Complex32::new(c.re - avg_real, c.im - avg_img)
        });

        connect!(fg, src > dc);
        dc
    } else {
        src
    };

    let delay = Delay::<Complex32>::new(16);
    connect!(fg, prev > delay);

    let complex_to_mag_2 = Apply::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    connect!(fg, prev > complex_to_mag_2 > float_avg);

    let mult_conj = Combine::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    connect!(fg, prev > in0.mult_conj.out > complex_avg;
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
    fft.set_tag_propagation(Box::new(copy_tag_propagation));
    let frame_equalizer = FrameEqualizer::new();
    let decoder = Decoder::new();
    let symbol_sink = WebsocketPmtSink::new(9002);
    connect!(fg, sync_long > fft > frame_equalizer > decoder;
        frame_equalizer.symbols | symbol_sink.in);

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    let udp1 = futuresdr::blocks::BlobToUdp::new("127.0.0.1:55555");
    let udp2 = futuresdr::blocks::BlobToUdp::new("127.0.0.1:55556");
    connect!(fg, decoder.rx_frames | message_pipe;
                 decoder.rx_frames | udp1;
                 decoder.rftap | udp2);

    let (_fg, _handle) = rt.start_sync(fg);
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
