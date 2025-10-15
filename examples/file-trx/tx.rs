use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Gain to apply
    #[clap(short, long, default_value_t = 0.0)]
    gain: f64,

    /// Center frequency
    #[clap(short, long, default_value_t = 100_000_000.0)]
    frequency: f64,

    /// Sample rate
    #[clap(short, long, default_value_t = 1000000.0)]
    sample_rate: f64,

    /// Seify args
    #[clap(short, long, default_value = "")]
    args: String,

    /// File source to load
    #[clap(short, long)]
    input: String,

    /// Input file format, automatically determined from filename if not specified
    #[clap(long)]
    format_in: Option<String>,

    /// Repeat
    #[clap(short, long, default_value_t = false)]
    repeat: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut fg = Flowgraph::new();

    let format = args
        .format_in
        .or_else(|| {
            let parts: Vec<_> = args.input.split('.').collect();
            Some(parts[parts.len() - 1].to_string())
        })
        .expect("Input format could not be determined!");

    let src: BlockId = match format.as_str() {
        "cs8" => {
            let src = FileSource::<Complex<i8>>::new(args.input, args.repeat);
            let typecvt = Apply::<_, _, _>::new(|i: &Complex<i8>| Complex {
                re: i.re as f32 / 128.,
                im: i.im as f32 / 128.,
            });
            connect!(fg, src > typecvt);
            typecvt.into()
        }
        "cf32" => {
            let src = FileSource::<Complex32>::new(args.input, args.repeat);
            connect!(fg, src);
            src.into()
        }
        _ => {
            panic!("Unrecognized input format {format}");
        }
    };

    let snk = Builder::new(args.args)?
        .frequency(args.frequency)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .build_sink()?;
    let snk = fg.add_block(snk);

    fg.connect_dyn(src, "output", snk, "inputs[0]")?;

    Runtime::new().run(fg)?;

    Ok(())
}
