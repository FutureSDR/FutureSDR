use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::AGC;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ConsoleSink;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::log::info;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use cw::CWAlphabet;
use cw::BBToCWBuilder;
use cw::CWToCharBuilder;
use futuredsp::firdes;

#[derive(Parser, Debug)]
struct Args {
    /// Send message on given frequency.
    #[clap(short, long, default_value_t = 1210.0e6)]
    freq: f32,
    /// SDR gain.
    #[clap(short, long, default_value_t = 40.0)]
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

    // Design bandpass filter for the middle tone
    let cutoff = (440.0) as f64 / 48_000. as f64;
    let transition_bw = 100.0 / 48_000. as f64;
    let max_ripple = 0.01;

    let filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    info!("Filter has {} taps", filter_taps.len());

    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(args.freq as f64)
        .sample_rate(args.sample_rate as f64)
        .gain(args.gain as f64)
        .build();
    let conv = Apply::new(|x: &Complex32| x.re);
    let agc = AGC::<f32>::new(0.0, 1.0);
    let lowpass = FirBuilder::new::<f32, f32, _, _>(filter_taps);

    let iq_to_cw = BBToCWBuilder::new().accuracy(100).sample_rate(args.sample_rate as f64).dot_length(dot_length as f64).build();
    let cw_snk = ConsoleSink::<CWAlphabet>::new(" ");
    let cw_to_char = CWToCharBuilder::new().build();
    let char_snk = ConsoleSink::<char>::new("");

    connect!(fg,
        src > conv > agc > lowpass > iq_to_cw > cw_snk;
        iq_to_cw > cw_to_char > char_snk;
    );

    Runtime::new().run(fg)?;
    Ok(())
}
