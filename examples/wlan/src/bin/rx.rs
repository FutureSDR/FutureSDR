use clap::Parser;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::FileSource;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamInput;
use futuresdr::runtime::StreamOutput;

use wlan::Decoder;
use wlan::Delay;
use wlan::FftShift;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

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

    // ========================================
    // Receiver
    // ========================================
    let src = fg.add_block(FileSource::<Complex32>::new("data/bpsk-1-2-15db.cf32"));
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

    let fft_shift = fg.add_block(FftShift::<Complex32>::new());
    fg.connect_stream(fft, "out", fft_shift, "in")?;

    let frame_equalizer = fg.add_block(FrameEqualizer::new());
    fg.connect_stream(fft, "out", frame_equalizer, "in")?;

    let decoder = fg.add_block(Decoder::new());
    fg.connect_stream(frame_equalizer, "out", decoder, "in")?;

    // Debug
    // let tag_debug = fg.add_block(TagDebug::<Complex32>::new("equalizer out"));
    // fg.connect_stream(fft, "out", tag_debug, "in")?;

    // let snk = fg.add_block(NullSink::<u8>::new());
    // fg.connect_stream(frame_equalizer, "out", snk, "in")?;

    // let float_to_complex = fg.add_block(Apply::new(|i: &f32| Complex32::new(*i, 0.0)));
    let file_snk = fg.add_block(FileSink::<Complex32>::new("/tmp/fs.cf32"));
    // fg.connect_stream(divide_mag, "out", float_to_complex, "in")?;
    fg.connect_stream(fft_shift, "out", file_snk, "in")?;

    let _ = Runtime::new().run(fg)?;
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
