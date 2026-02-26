use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::WebsocketPmtSink;
use futuresdr::prelude::*;
use std::time;

use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Antenna
    #[clap(short, long, default_value = "wlan-100.cf32")]
    file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let src = FileSource::<Complex32>::new(&args.file, false);
    connect!(fg, src);

    let delay = Delay::<Complex32>::new(16);
    connect!(fg, src > delay);

    let complex_to_mag_2 = Apply::<_, _, _>::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    connect!(fg, src > complex_to_mag_2);
    connect!(fg, complex_to_mag_2 > float_avg);

    let mult_conj = Combine::<_, _, _, _>::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    connect!(fg, src > in0.mult_conj);
    connect!(fg, mult_conj > complex_avg;
                 delay > in1.mult_conj);

    let divide_mag = Combine::<_, _, _, _>::new(|a: &Complex32, b: &f32| a.norm() / b);
    connect!(fg, complex_avg > in0.divide_mag; float_avg > in1.divide_mag);

    let sync_short: SyncShort = SyncShort::new();
    connect!(fg, delay > in_sig.sync_short;
                 complex_avg > in_abs.sync_short;
                 divide_mag > in_cor.sync_short);

    let sync_long: SyncLong = SyncLong::new();
    connect!(fg, sync_short > sync_long);

    let fft: Fft = Fft::new(64);
    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    let decoder = Decoder::new();
    let symbol_sink = WebsocketPmtSink::new(9002);
    connect!(fg, sync_long > fft > frame_equalizer > decoder;
        frame_equalizer.symbols | r#in.symbol_sink);

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    connect!(fg, decoder.rx_frames | message_pipe);

    let now = time::Instant::now();
    let (_fg, _handle) = rt.start_sync(fg)?;
    let elapsed = now.elapsed();
    println!("{}, {}", args.file, elapsed.as_secs_f64());

    rt.block_on(async move {
        let mut c = 0;
        while let Some(x) = rx_frame.next().await {
            match x {
                Pmt::Blob(data) => {
                    c += 1;
                    println!("received frame {} ({:?} bytes)", c, data.len());
                }
                t => {
                    println!("{:?}", t);
                    break
                },
            }
        }
    });

    Ok(())
}
