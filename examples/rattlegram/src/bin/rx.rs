use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::audio::AudioSource;
use futuresdr::blocks::audio::FileSource;
use futuresdr::prelude::*;

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

    let snk: DecoderBlock = DecoderBlock::new();

    if let Some(f) = args.file {
        let src: FileSource = FileSource::new(&f);
        connect!(fg, src > snk);
    } else {
        let src: AudioSource = AudioSource::new(48000, 1);
        connect!(fg, src > snk);
    };

    Runtime::new().run(fg)?;

    Ok(())
}
