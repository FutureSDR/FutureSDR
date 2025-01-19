use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::audio::*;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Split;
use futuresdr::futuredsp::firdes;
use futuresdr::futuredsp::windows::hamming;
use futuresdr::hound::SampleFormat;
use futuresdr::hound::WavSpec;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::f32::consts::TAU;
use std::path::Path;

#[derive(Clone, Debug)]
enum Mode {
    Lsb,
    Usb,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Mode::Lsb => write!(f, "LSB"),
            Mode::Usb => write!(f, "USB"),
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "LSB" => Ok(Mode::Lsb),
            "USB" => Ok(Mode::Usb),
            _ => Err("Not a valid mode".to_owned()),
        }
    }
}

#[derive(Parser)]
struct Cli {
    input: String,
    output: String,

    #[arg(short, long, default_value_t = Mode::Lsb)]
    mode: Mode,

    #[clap(short, long, default_value_t = 53e3)]
    frequency: f32,

    #[clap(long, default_value_t = 256_000)]
    sample_rate: u32,

    #[clap(long, default_value_t = 3000.0)]
    audio_bandwidth: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut fg = Flowgraph::new();

    let source = FileSource::new(&cli.input);
    assert!(
        source.kernel.channels() == 1,
        "Input audio must be mono but found {} channels",
        source.kernel.channels()
    );

    let audio_rate = source.kernel.sample_rate() as f64;
    let file_rate = cli.sample_rate;

    // Using a bandpass instead, can help to tame low frequencies bleeding
    // ouside of the chosen bandwidth.
    let taps = firdes::kaiser::lowpass(cli.audio_bandwidth / audio_rate, 350.0 / audio_rate, 0.05);
    let lowpass = FirBuilder::new::<f32, f32, _>(taps);

    let split = Split::new(move |v: &f32| (*v, *v));

    // Phase transformation by 90°.
    let window = hamming(167, false);
    let taps = firdes::hilbert(window.as_slice());
    let hilbert = FirBuilder::new::<f32, f32, _>(taps);

    // Match the delay caused by the phase transformation.
    let delay = Delay::<f32>::new(window.len() as isize / -2);

    let to_complex = Combine::new(move |i: &f32, q: &f32| match cli.mode {
        Mode::Lsb => Complex32::new(*i, *q * -1.0),
        Mode::Usb => Complex32::new(*i, *q),
    });

    let resampler =
        FirBuilder::resampling::<Complex32, Complex32>(file_rate as usize, audio_rate as usize);

    let mut osc = Complex32::new(1.0, 0.0);
    let shift = Complex32::from_polar(1.0, TAU * cli.frequency / file_rate as f32);
    let mixer = Apply::new(move |v: &Complex32| {
        osc *= shift;
        v * osc
    });

    let to_i16_iq = ApplyNM::<_, _, _, 1, 2>::new(move |i: &[Complex32], o: &mut [i16]| {
        o[0] = (i[0].re * 0.9 * i16::MAX as f32) as i16;
        o[1] = (i[0].im * 0.9 * i16::MAX as f32) as i16;
    });

    let sink = WavSink::<i16>::new(
        Path::new(format!("{}.wav", cli.output).as_str()),
        WavSpec {
            channels: 2,
            sample_rate: file_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    );

    // Adjust amplitude to levels expected by `receiver`.
    let file_level = Apply::new(|v: &Complex32| v * 2.0 / 0.0001);
    let dat = FileSink::<Complex32>::new(format!("{}.dat", cli.output));

    connect!(fg,
        source > lowpass > split;
        split.out0 > delay > to_complex.in0;
        split.out1 > hilbert > to_complex.in1;
        to_complex > resampler > mixer > to_i16_iq > sink;
        mixer > file_level > dat;
    );

    Runtime::new().run(fg)?;

    Ok(())
}
