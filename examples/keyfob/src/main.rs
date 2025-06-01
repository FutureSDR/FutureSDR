use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::seify::Builder;
use futuresdr::futuredsp::firdes;
use futuresdr::futuredsp::windows;
use futuresdr::prelude::*;

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

    let resamp = match args.file {
        Some(file) => {
            let file = FileSource::<Complex32>::new(file, false);
            let resamp =
                FirBuilder::resampling::<Complex32, Complex32>(250000, args.sample_rate as usize);
            connect!(fg, file > resamp);
            resamp
        }
        None => {
            let src = Builder::new(args.args)?
                .sample_rate(args.sample_rate)
                .frequency(args.freq)
                .gain(args.gain)
                .build_source()?;
            let resamp =
                FirBuilder::resampling::<Complex32, Complex32>(250000, args.sample_rate as usize);
            connect!(fg, src.outputs[0] > resamp);
            resamp
        }
    };

    let complex_to_mag = Apply::<_, _, _>::new(|i: &Complex32| -> f32 { i.norm_sqr() });

    let mut cur = 0.0;
    let alpha = 0.0001;
    let alpha_inv = 1.0 - alpha;
    let avg = Apply::<_, _, _>::new(move |x: &f32| -> f32 {
        cur = cur * alpha_inv + *x * alpha;
        *x - cur
    });

    let taps = firdes::lowpass::<f32>(15e3 / 250e3, &windows::hamming(128, false));
    let low_pass = FirBuilder::fir::<f32, f32, _>(taps);

    let slice = Apply::<_, _, _>::new(move |i: &f32| -> u8 {
        if *i > 0.0 { 1 } else { 0 }
    });

    let decoder = Decoder::new();

    connect!(fg, resamp > complex_to_mag > avg > low_pass > slice > decoder);

    Runtime::new().run(fg)?;

    Ok(())
}
