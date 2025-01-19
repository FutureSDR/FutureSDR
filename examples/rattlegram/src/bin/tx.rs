use anyhow::bail;
use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::VectorSource;
use futuresdr::hound;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use rattlegram::Encoder;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long)]
    file: Option<String>,
    #[clap(short, long, default_value_t = 5)]
    noise_symbols: u64,
    #[clap(short, long, default_value = "DF1BBL")]
    call_sign: String,
    #[clap(short, long, default_value = "Hello World!")]
    payload: String,
    #[clap(long, default_value_t = 2000)]
    carrier_frequency: usize,
    #[clap(long, default_value_t = false)]
    fancy_header: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let payload = args.payload.into_bytes();
    let call_sign = args.call_sign.into_bytes();

    if payload.len() > Encoder::MAX_BITS {
        bail!(
            "payload too long ({}, {} allowed)",
            payload.len(),
            Encoder::MAX_BITS / 8
        );
    }
    if call_sign.len() > 9 {
        bail!("call_sign too long ({}, {} allowed)", call_sign.len(), 9);
    }

    let mut e = Encoder::new();

    let sig = e.encode(
        payload.as_slice(),
        call_sign.as_slice(),
        args.carrier_frequency,
        args.noise_symbols,
        args.fancy_header,
    );

    if let Some(f) = args.file {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(f, spec).unwrap();
        sig.into_iter()
            .for_each(|s| writer.write_sample(s).unwrap());
    } else {
        let mut fg = Flowgraph::new();
        let src = VectorSource::new(sig);
        let snk = AudioSink::new(48000, 1);
        connect!(fg, src > snk);

        Runtime::new().run(fg)?;
    }

    Ok(())
}
