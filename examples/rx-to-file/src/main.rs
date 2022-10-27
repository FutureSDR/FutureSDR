use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Head;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::{FileSink, FileSource};
use futuresdr::num_complex::{Complex, Complex32};
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::time::Instant;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Gain to apply to the soapy source
    #[clap(short, long, default_value_t = 0.0)]
    gain: f64,

    /// Center frequency
    #[clap(short, long, default_value_t = 100_000_000.0)]
    frequency: f64,

    /// Sample rate
    #[clap(short, long, default_value_t = 1000000.0)]
    rate: f64,

    /// Soapy source to use as a source
    #[clap(long)]
    soapy: Option<String>,

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

    let src = match (args.soapy, args.input) {
        (Some(_), Some(_)) => {
            panic!("Cannot specify both soapy source and input file");
        }
        (Some(soapy), None) => fg.add_block(
            SoapySourceBuilder::new()
                .freq(args.frequency)
                .sample_rate(args.rate)
                .gain(args.gain)
                .filter(soapy)
                .build(),
        ),
        (None, Some(input)) => {
            let format = args
                .format_in
                .or_else(|| {
                    let parts: Vec<_> = input.split('.').collect();
                    Some(parts[parts.len() - 1].to_string())
                })
                .expect("Input format could not be determined!");
            match format.as_str() {
                "cs8" => {
                    let src = fg.add_block(FileSource::<Complex<i8>>::new(input, false));
                    let typecvt = fg.add_block(Apply::new(|i: &Complex32| Complex {
                        re: i.re / 127.,
                        im: i.im / 127.,
                    }));
                    fg.connect_stream(src, "out", typecvt, "in")?;
                    typecvt
                }
                _ => {
                    panic!("Unrecognized input format {format}");
                }
            }
        }
        (None, None) => {
            panic!("Must specify one of soapy source or input file");
        }
    };

    let src = if let Some(samples) = args.samples {
        let sample_counter = fg.add_block(Head::<Complex<f32>>::new(samples));
        fg.connect_stream(src, "out", sample_counter, "in")?;
        sample_counter
    } else {
        src
    };

    let mut last_clip_warning = Instant::now();
    let mut last_power_print = Instant::now();
    let mut avgmag = 0.0;
    let mut maxmag = 0.0;
    let powermeter = fg.add_block(Apply::new(move |i: &Complex32| {
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

    fg.connect_stream(src, "out", powermeter, "in")?;

    let format = args
        .format_out
        .or_else(|| {
            let parts: Vec<_> = args.out.split('.').collect();
            Some(parts[parts.len() - 1].to_string())
        })
        .expect("Output format could not be determined!");
    match format.as_str() {
        "cs8" => {
            let typecvt = fg.add_block(Apply::new(|i: &Complex32| Complex {
                re: (i.re * 127.) as i8,
                im: (i.im * 127.) as i8,
            }));
            let sink = fg.add_block(FileSink::<Complex<i8>>::new(&args.out));
            fg.connect_stream(powermeter, "out", typecvt, "in")?;
            fg.connect_stream(typecvt, "out", sink, "in")?;
        }
        "cf32" => {
            let sink = fg.add_block(FileSink::<Complex<f32>>::new(&args.out));
            fg.connect_stream(powermeter, "out", sink, "in")?;
        }
        format => {
            panic!("Unknown format {format}! (known formats: cs8, cf32)");
        }
    }

    Runtime::new().run(fg)?;

    Ok(())
}
