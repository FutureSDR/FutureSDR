use adsb_demod::DEMOD_SAMPLE_RATE;
use adsb_demod::Decoder;
use adsb_demod::Demodulator;
use adsb_demod::PreambleDetector;
use adsb_demod::Tracker;
use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::seify::Builder;
use futuresdr::num_integer;
use futuresdr::prelude::*;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Antenna
    #[arg(long)]
    antenna: Option<String>,
    /// Seify Args
    #[arg(short, long)]
    args: Option<String>,
    /// Gain
    #[arg(short, long, default_value_t = 30.0)]
    gain: f64,
    /// Sample rate
    #[arg(short, long, default_value_t = 2.2e6, value_parser = sample_rate_parser)]
    sample_rate: f64,
    /// Preamble detection threshold
    #[arg(short, long, default_value_t = 10.0)]
    preamble_threshold: f32,
    /// Use a file instead of a device
    #[arg(short, long)]
    file: Option<String>,
    /// Remove aircrafts when no packets have been received for the specified number of seconds
    #[arg(short, long)]
    lifetime: Option<u64>,
}

fn sample_rate_parser(sample_rate_str: &str) -> Result<f64, String> {
    let sample_rate: f64 = sample_rate_str
        .parse()
        .map_err(|_| format!("`{sample_rate_str}` is not a valid sample rate"))?;
    // Sample rate must be at least 2 MHz
    if sample_rate < 2e6 {
        Err("Sample rate must be at least 2 MHz".to_string())
    } else {
        Ok(sample_rate)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut fg = Flowgraph::new();
    futuresdr::runtime::init();

    let src: BlockId = match args.file {
        Some(ref f) => {
            let file_src_block = FileSource::<Complex32>::new(f, false);
            let throttle_block = Throttle::<Complex32>::new(args.sample_rate);
            connect!(fg, file_src_block > throttle_block);
            throttle_block.into()
        }
        None => {
            // Load seify source
            let src = Builder::new(args.args)?
                .frequency(1090e6)
                .sample_rate(args.sample_rate)
                .gain(args.gain)
                .antenna(args.antenna)
                .build_source()?;

            fg.add_block(src).into()
        }
    };

    // Change sample rate to our demodulator sample rate.
    // Using a sample rate higher than the signal bandwidth allows
    // us to use a simple symbol synchronization mechanism and have
    // more clear symbol transitions.
    let gcd = num_integer::gcd(args.sample_rate as usize, DEMOD_SAMPLE_RATE);
    let interp = DEMOD_SAMPLE_RATE / gcd;
    let decim = args.sample_rate as usize / gcd;
    if interp > 100 || decim > 100 {
        warn!(
            "Warning: Interpolation/decimation factor is large. \
             Use a sampling frequency that is a divisor of {DEMOD_SAMPLE_RATE} for the best performance."
        );
    }
    let interp_block = fg.add_block(FirBuilder::resampling::<Complex32, Complex32>(
        interp, decim,
    ));
    if args.file.is_some() {
        fg.connect_dyn(src, "output", &interp_block, "input")?;
    } else {
        fg.connect_dyn(src, "outputs[0]", &interp_block, "input")?;
    }

    let complex_to_mag_2: Apply<_, _, _> = Apply::new(|i: &Complex32| i.norm_sqr());
    let nf_est_block = FirBuilder::fir::<f32, f32, _>(vec![1.0f32 / 32.0; 32]);
    let preamble_taps: Vec<f32> =
        PreambleDetector::<DefaultCpuReader<f32>>::preamble_correlator_taps();
    let preamble_corr_block = FirBuilder::fir::<f32, f32, _>(preamble_taps);
    let preamble_detector = PreambleDetector::new(args.preamble_threshold);
    let adsb_demod = Demodulator::new();
    let adsb_decoder = Decoder::new(false);
    let tracker = match args.lifetime {
        Some(s) => Tracker::with_pruning(Duration::from_secs(s)),
        None => Tracker::new(),
    };
    connect!(fg, interp_block > complex_to_mag_2 > nf_est_block;
    complex_to_mag_2 > preamble_corr_block;
    complex_to_mag_2 > in_samples.preamble_detector;
    nf_est_block > in_nf.preamble_detector;
    preamble_corr_block > in_preamble_cor.preamble_detector;
    preamble_detector > adsb_demod | adsb_decoder | tracker
    );

    println!("Please open the map in the browser: http://127.0.0.1:1337/");
    Runtime::new().run(fg)?;

    Ok(())
}
