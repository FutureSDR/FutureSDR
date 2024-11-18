use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::AudioSource;
use futuresdr::blocks::ChannelSource;
use futuresdr::futures::channel::mpsc;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use rattlegram::DecoderBlock;
use rattlegram::Encoder;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long, default_value_t = 5)]
    noise_symbols: u64,
    #[clap(short, long, default_value = "DF1BBL")]
    call_sign: String,
    #[clap(long, default_value_t = 2000)]
    carrier_frequency: usize,
    #[clap(long, default_value_t = false)]
    fancy_header: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    // RX
    let src = AudioSource::new(48000, 1);
    let snk = DecoderBlock::new();
    connect!(fg, src > snk);

    // TX
    let (mut tx, rx) = mpsc::channel(10);
    let src = ChannelSource::<f32>::new(rx);
    let snk = AudioSink::new(48000, 1);
    connect!(fg, src > snk);

    let rt = Runtime::new();
    let (_task, _handle) = rt.start_sync(fg);

    // Keep asking user for a new frequency and a new sample rate
    loop {
        println!("Enter Message");
        let mut input = String::new(); // Input buffer
        std::io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");

        if input.len() > 100 {
            println!("Message too long {}", input.len());
        }

        let mut e = Encoder::new();

        let sig = e.encode(
            input.as_bytes(),
            args.call_sign.as_bytes(),
            args.carrier_frequency,
            args.noise_symbols,
            args.fancy_header,
        );
        tx.try_send(sig.into())?;
    }
}
