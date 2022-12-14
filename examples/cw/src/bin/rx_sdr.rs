// cargo build --bin rx-sdr --features="soapy zmq" --release

use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::AGCBuilder;
use futuresdr::blocks::Apply;
//use futuresdr::blocks::Throttle;
//use futuresdr::blocks::ApplyIntoIter;
//use futuresdr::blocks::VectorSource;
use futuresdr::blocks::ConsoleSink;
//use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::SoapySourceBuilder;
//use futuresdr::blocks::zeromq::PubSinkBuilder;
//use futuresdr::log::info;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

//use cw::char_to_bb;
//use cw::msg_to_cw;
use cw::CWAlphabet;
use cw::BBToCWBuilder;
use cw::CWToCharBuilder;
//use futuredsp::firdes;

#[derive(Parser, Debug)]
struct Args {
    /// Send message on given frequency.
    #[clap(short, long, default_value_t = 1_210_000_800.0)]
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
}

fn main() -> Result<()> {
    let args = Args::parse();

    /*let dot_length = args.sample_rate * 60.0 / (50.0 * args.wpm);

    // Design bandpass filter for the middle tone
    let cutoff = 440.0 / 48_000.0;
    let transition_bw = 100.0 / 48_000.0;
    let max_ripple = 0.01;

    let filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    info!("Filter has {} taps", filter_taps.len());*/

    let samles_per_dot = args.sample_rate * 60.0 / (50.0 * args.wpm);
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .filter("driver=rtlsdr")
        .build();

    /*let msg: Vec<char> = "SOS SOS".trim().to_uppercase().chars().collect();
    info!(
        "encoded message: {}",
        msg_to_cw(&msg)
            .iter()
            .map(|x| format!("{}", x))
            .collect::<String>()
    );
    let msg = [vec![' '], msg, vec![' ']].concat();

    let src = VectorSource::<char>::new(msg);
    let encode = ApplyIntoIter::<_, _, _>::new(char_to_bb(samles_per_dot as usize));*/

    let conv = Apply::new(|x: &Complex32| (x.re.powi(2) + x.im.powi(2)).sqrt());
    //let throttle = Throttle::<f32>::new(args.sample_rate);
    //let zmq_snk1 = PubSinkBuilder::<f32>::new().address("tcp://127.0.0.1:50001").build();
    let agc = AGCBuilder::<f32>::new().reference_power(1.0).build();
    //let zmq_snk2 = PubSinkBuilder::<f32>::new().address("tcp://127.0.0.1:50002").build();
    let iq_to_cw = BBToCWBuilder::new().accuracy(70).samples_per_dot(samles_per_dot as usize).build();
    let _cw_snk = ConsoleSink::<CWAlphabet>::new(" ");
    let cw_to_char = CWToCharBuilder::new().build();
    let char_snk = ConsoleSink::<char>::new("");

    connect!(fg,
        src > conv > agc > iq_to_cw;
        iq_to_cw > cw_to_char > char_snk;
    );

    Runtime::new().run(fg)?;
    Ok(())
}
