use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSource;
use futuresdr::blocks::audio::FileSource;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use rattlegram::DecoderBlock;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long)]
    file: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    let src = if let Some(f) = args.file {
        FileSource::new(&f)
    } else {
        AudioSource::new(48000, 1)
    };

    let snk = DecoderBlock::new();
    connect!(fg, src > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
