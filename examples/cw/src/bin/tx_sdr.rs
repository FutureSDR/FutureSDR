//cargo build --bin tx-sdr --features="soapy" --release

use clap::Parser;

use cw::char_to_bb;
use cw::msg_to_cw;
use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::log::info;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[derive(Parser, Debug)]
struct Args {
    /// Sets the message to convert.
    #[clap(short, long, default_value = "CQ CQ CQ FUTURESDR")]
    message: String,
    /// Send message on given frequency.
    #[clap(short, long, default_value_t = 1210.0e6)]
    freq: f32,
    /// SDR gain.
    #[clap(short, long, default_value_t = 36.4)]
    gain: f32,
    /// SDR sample rate.
    #[clap(short, long, default_value_t = 250000.0)]
    sample_rate: f32,
    /// Words per minute.
    #[clap(short, long, default_value_t = 12.0)]
    wpm: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let dot_length = args.sample_rate * 60.0 / (50.0 * args.wpm);

    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let msg: Vec<char> = args.message.trim().to_uppercase().chars().collect();
    info!(
        "encoded message: {}",
        msg_to_cw(&msg)
            .iter()
            .map(|x| format!("{}", x))
            .collect::<String>()
    );
    let msg = [vec![' '], msg, vec![' ']].concat();

    let src = VectorSource::<char>::new(msg);
    let encode = ApplyIntoIter::<_, _, _>::new(char_to_bb(dot_length as usize));
    let conv = Apply::new(|x: &f32| Complex32::new(*x, 0.0));
    let snk = SoapySinkBuilder::new()
        .freq(args.freq as f64)
        .sample_rate(args.sample_rate as f64)
        .gain(args.gain as f64)
        .filter("driver=bladerf")
        .build();

    connect!(
        fg, src > encode > conv;
        conv > snk;
    );

    Runtime::new().run(fg)?;
    Ok(())
}
