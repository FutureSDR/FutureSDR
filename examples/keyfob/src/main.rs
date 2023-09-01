use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::futuredsp::firdes;
use futuresdr::futuredsp::windows;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use keyfob::Decoder;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// File
    #[clap(long)]
    file: Option<String>,
    /// Sample Rate
    #[clap(short, long, default_value_t = 4e6)]
    sample_rate: f64,
    /// Seify Args
    #[clap(short, long)]
    args: Option<String>,
    /// Gain
    #[clap(short, long, default_value_t = 50.0)]
    gain: f64,
    /// Frequency
    #[clap(short, long, default_value_t = 434.45e6)]
    freq: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    let src = match args.file {
        Some(file) => FileSource::<Complex32>::new(file, false),
        None => {
            let mut src = SourceBuilder::new()
                .sample_rate(args.sample_rate)
                .frequency(args.freq)
                .gain(args.gain);
            if let Some(a) = args.args {
                src = src.args(a)?;
            }
            src.build()?
        }
    };

    let resamp = FirBuilder::new_resampling::<Complex32, Complex32>(1, 16);
    let complex_to_mag = Apply::new(|i: &Complex32| -> f32 { i.norm_sqr() });

    let mut cur = 0.0;
    let alpha = 0.0001;
    let alpha_inv = 1.0 - alpha;
    let avg = Apply::new(move |x: &f32| -> f32 {
        cur = cur * alpha_inv + *x * alpha;
        *x - cur
    });

    let taps = firdes::lowpass::<f32>(15e3 / 250e3, &windows::hamming(128, false));
    let low_pass = FirBuilder::new::<f32, f32, _, _>(taps);

    let slice = Apply::new(move |i: &f32| -> u8 {
        if *i > 0.0 {
            1
        } else {
            0
        }
    });

    let decoder = Decoder::new();

    // let to_complex = Apply::new(|f: &f32| Complex32::new(*f, 0.0));
    // let file_sink = futuresdr::blocks::FileSink::<Complex32>::new("out.cf32");

    connect!(fg, src > resamp > complex_to_mag > avg > low_pass > slice > decoder);

    Runtime::new().run(fg)?;

    Ok(())
}
