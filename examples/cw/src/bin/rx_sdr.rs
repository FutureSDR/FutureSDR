// cargo build --bin rx-sdr --features="soapy" --release

use clap::Parser;

use cw::BBToCWBuilder;
use cw::CWAlphabet;
use cw::CWToCharBuilder;
use futuresdr::anyhow::Result;
use futuresdr::blocks::AGCBuilder;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ConsoleSink;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[derive(Parser, Debug)]
struct Args {
    /// Send message on given frequency.
    #[clap(short, long, default_value_t = 1210.0e6)]
    freq: f64,
    /// SDR gain.
    #[clap(short, long, default_value_t = 36.4)]
    gain: f64,
    /// SDR sample rate.
    #[clap(short, long, default_value_t = 250000.0)]
    sample_rate: f64,
    /// Words per minute.
    #[clap(short, long, default_value_t = 12.0)]
    wpm: f64,
    /// Minimum power level to activate AGC
    #[clap(short, long, default_value_t = 0.0)] //0.035
    squelch: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let resampling_factor = 50;
    let samles_per_dot = (args.sample_rate * 60.0 / (50.0 * args.wpm)) / resampling_factor as f64;
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .filter("driver=rtlsdr")
        .build();

    let resamp = FirBuilder::new_resampling::<Complex32, Complex32>(1, resampling_factor);
    let conv = Apply::new(|x: &Complex32| (x.re.powi(2) + x.im.powi(2)).sqrt()); // x.re.abs()
    let agc = AGCBuilder::<f32>::new().adjustment_rate(0.1).reference_power(1.0).squelch(args.squelch).build();
    let iq_to_cw = BBToCWBuilder::new().accuracy(80).samples_per_dot(samles_per_dot as usize).build();
    let _cw_snk = ConsoleSink::<CWAlphabet>::new(" ");
    let cw_to_char = CWToCharBuilder::new().build();
    let char_snk = ConsoleSink::<char>::new("");

    connect!(fg,
        src > resamp > conv > agc > iq_to_cw;
        //iq_to_cw > cw_snk;
        iq_to_cw > cw_to_char > char_snk;
    );

    Runtime::new().run(fg)?;
    Ok(())
}
