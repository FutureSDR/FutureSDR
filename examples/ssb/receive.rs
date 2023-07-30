use clap::Parser;

use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::num_integer::gcd;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use num_complex::Complex32;

// Inspired by https://wiki.gnuradio.org/index.php/Simulation_example:_Single_Sideband_transceiver

#[derive(Parser, Debug)]
struct Args {
    /// file sample rate
    #[clap(long, default_value_t = 256_000)]
    file_rate: u32,

    /// file to use as a source
    #[clap(short, long, default_value = "ssb_lsb_256k_complex2.dat")]
    filename: String,

    /// Audio Rate
    #[clap(short, long)]
    audio_rate: Option<u32>,

    /// center frequency
    /// explanation in http://www.csun.edu/~skatz/katzpage/sdr_project/sdr/grc_tutorial4.pdf
    #[clap(short, long, default_value_t = 51_500)]
    center_freq: i32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration {args:?}");

    let file_rate = args.file_rate;

    let audio_rate = if let Some(r) = args.audio_rate {
        r
    } else {
        let mut audio_rates = AudioSink::supported_sample_rates();
        assert!(!audio_rates.is_empty());
        audio_rates.sort_by_key(|a| std::cmp::Reverse(gcd(*a, file_rate)));
        println!("Supported Audio Rates {audio_rates:?}");
        audio_rates[0]
    };
    println!("Selected Audio Rate {audio_rate:?}");
    let mut fg = Flowgraph::new();

    let center_freq = args.center_freq;

    // To be downloaded from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip
    let file_name = args.filename;
    let mut src = FileSource::<Complex32>::new(&file_name, true);
    src.set_instance_name(format!("File {file_name}"));

    const FILE_LEVEL_ADJUSTMENT: f32 = 0.0001;
    let mut osc = Complex32::new(1.0, 0.0);
    let shift = Complex32::from_polar(
        1.0,
        -2.0 * std::f32::consts::PI * (center_freq as f32) / (file_rate as f32),
    );
    let mut freq_xlating = Apply::new(move |v: &Complex32| {
        osc *= shift;
        v * osc * FILE_LEVEL_ADJUSTMENT
    });
    freq_xlating.set_instance_name(format!("freq_xlating {center_freq}"));

    let mut low_pass_filter =
        FirBuilder::new_resampling::<Complex32, Complex32>(audio_rate as usize, file_rate as usize);
    low_pass_filter.set_instance_name(format!("resampler {audio_rate} {file_rate}"));

    const VOLUME_ADJUSTEMENT: f32 = 0.5;
    const MID_AUDIO_SPECTRUM_FREQ: u32 = 1500;
    let mut osc = Complex32::new(1.0, 0.0);
    let shift = Complex32::from_polar(
        1.0,
        2.0 * std::f32::consts::PI * (MID_AUDIO_SPECTRUM_FREQ as f32) / (audio_rate as f32),
    );
    let mut weaver_ssb_decode = Apply::new(move |v: &Complex32| {
        osc *= shift;
        let term1 = v.re * osc.re;
        let term2 = v.im * osc.im;
        VOLUME_ADJUSTEMENT * (term1 + term2) // substraction for LSB, addition for USB
    });
    weaver_ssb_decode.set_instance_name("Weaver SSB decoder");

    let snk = AudioSink::new(audio_rate, 1);

    let src = fg.add_block(src);
    let freq_xlating = fg.add_block(freq_xlating);
    let low_pass_filter = fg.add_block(low_pass_filter);
    let weaver_ssb_decode = fg.add_block(weaver_ssb_decode);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", freq_xlating, "in")?;
    fg.connect_stream(freq_xlating, "out", low_pass_filter, "in")?;
    fg.connect_stream(low_pass_filter, "out", weaver_ssb_decode, "in")?;
    fg.connect_stream(weaver_ssb_decode, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
