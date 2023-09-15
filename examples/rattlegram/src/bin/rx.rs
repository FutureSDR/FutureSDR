use clap::Parser;
use futuresdr::anyhow::anyhow;
use futuresdr::anyhow::Result;
use std::fs::File;
use std::path::Path;

use rattlegram::Decoder;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long, default_value = "out.wav")]
    file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut inp_file = File::open(Path::new(&args.file))?;
    let (_, data) = wav::read(&mut inp_file)?;
    let samples = data.try_into_thirty_two_float();
    let samples = samples.or(Err(anyhow!("failed to convert")));

    println!("samples {:?}", samples);


    let decoder = Decoder::new();

    Ok(())
}
