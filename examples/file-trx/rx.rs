use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::Head;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use std::time::Instant;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Gain to apply to the source
    #[clap(short, long, default_value_t = 0.0)]
    gain: f64,

    /// Center frequency
    #[clap(short, long, default_value_t = 100_000_000.0)]
    frequency: f64,

    /// Sample rate
    #[clap(short, long, default_value_t = 1000000.0)]
    rate: f64,

    /// Seify source args
    #[clap(short, long, default_value = "")]
    args: String,

    /// File source to load
    #[clap(long)]
    input: Option<String>,

    /// Input file format, automatically determined from filename if not specified
    #[clap(long)]
    format_in: Option<String>,

    /// File to dump to
    #[clap(long)]
    out: String,

    /// Format to dump to. Will be automatically determined from the filename
    /// if not specified.
    #[clap(long)]
    format_out: Option<String>,

    /// Number of samples to record. Continuous recording if not specified.
    #[clap(long)]
    samples: Option<u64>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut fg = Flowgraph::new();

    let (src, output_name): (BlockId, _) = if let Some(input) = args.input {
        let format = args
            .format_in
            .or_else(|| {
                let parts: Vec<_> = input.split('.').collect();
                Some(parts[parts.len() - 1].to_string())
            })
            .expect("Input format could not be determined!");
        match format.as_str() {
            "cs8" => {
                let src = FileSource::<Complex<i8>>::new(input, false);
                let typecvt = Apply::<_, _, _>::new(|i: &Complex<i8>| Complex {
                    re: i.re as f32 / 127.,
                    im: i.im as f32 / 127.,
                });
                connect!(fg, src > typecvt);
                (typecvt.into(), "output")
            }
            _ => {
                panic!("Unrecognized input format {format}");
            }
        }
    } else {
        (
            fg.add_block(
                Builder::new(args.args)?
                    .frequency(args.frequency)
                    .sample_rate(args.rate)
                    .gain(args.gain)
                    .build_source()?,
            )
            .into(),
            "outputs[0]",
        )
    };

    let (src, output_name) = if let Some(samples) = args.samples {
        let sample_counter = fg.add_block(Head::<Complex<f32>>::new(samples)).into();
        fg.connect_dyn(src, output_name, sample_counter, "input")?;
        (sample_counter, "output")
    } else {
        (src, output_name)
    };

    let mut last_clip_warning = Instant::now();
    let mut last_power_print = Instant::now();
    let mut avgmag = 0.0;
    let mut maxmag = 0.0;
    let powermeter = fg.add_block(Apply::<_, _, _>::new(move |i: &Complex32| {
        let norm = i.norm();
        if norm > 0.95 && last_clip_warning.elapsed().as_secs_f32() > 0.1 {
            last_clip_warning = Instant::now();
            eprintln!("Possible clipping!");
        }
        avgmag = avgmag * 0.9999 + norm * 0.0001;
        if norm > maxmag {
            maxmag = norm;
        }
        if last_power_print.elapsed().as_secs_f32() > 2.0 {
            println!("Average/max signal magnitudes: {avgmag:.4}/{maxmag:.4}");
            maxmag = 0.0;
            last_power_print = Instant::now();
        }
        *i
    }));

    fg.connect_dyn(src, output_name, &powermeter, "input")?;

    let format = args
        .format_out
        .or_else(|| {
            let parts: Vec<_> = args.out.split('.').collect();
            Some(parts[parts.len() - 1].to_string())
        })
        .expect("Output format could not be determined!");
    match format.as_str() {
        "cs8" => {
            let typecvt = Apply::<_, _, _>::new(|i: &Complex32| Complex {
                re: (i.re * 127.) as i8,
                im: (i.im * 127.) as i8,
            });
            let sink = FileSink::<Complex<i8>>::new(&args.out);
            connect!(fg, powermeter > typecvt > sink);
        }
        "cf32" => {
            let sink = FileSink::<Complex<f32>>::new(&args.out);
            connect!(fg, powermeter > sink);
        }
        format => {
            panic!("Unknown format {format}! (known formats: cs8, cf32)");
        }
    }

    Runtime::new().run(fg)?;

    Ok(())
}
