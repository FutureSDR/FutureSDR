use clap::Parser;

use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::num_integer::gcd;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use num_complex::Complex;

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
    println!("Configuration {:?}", args);

    let file_rate = args.file_rate as u32;

    let audio_rate = if let Some(r) = args.audio_rate {
        r
    } else {
        let mut audio_rates = AudioSink::supported_sample_rates();
        assert!(!audio_rates.is_empty());
        audio_rates.sort_by_key(|a| std::cmp::Reverse(gcd(*a, file_rate)));
        println!("Supported Audio Rates {:?}", audio_rates);
        audio_rates[0]
    };
    println!("Selected Audio Rate {:?}", audio_rate);
    let mut fg = Flowgraph::new();

    let center_freq = args.center_freq;

    // To be downloaded from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip
    let file_name = args.filename;
    let mut src = FileSource::<Complex<f32>>::new(&file_name, true);
    src.set_instance_name(format!("File {}", file_name));

    const FILE_LEVEL_ADJUSTEMENT: f32 = 0.0001;
    let mut xlating_local_oscillator_index: u32 = 0;
    let fwt0: f32 = -2.0 * std::f32::consts::PI * (center_freq as f32) / (file_rate as f32);
    let mut freq_xlating = Apply::new(move |v: &Complex<f32>| {
        let lo_v = Complex::<f32>::new(0.0, (xlating_local_oscillator_index as f32) * fwt0).exp();
        xlating_local_oscillator_index = (xlating_local_oscillator_index + 1) % file_rate;
        FILE_LEVEL_ADJUSTEMENT * v * lo_v
    });
    freq_xlating.set_instance_name(&format!("freq_xlating {}", center_freq));

    // low_pass_filter.set_instance_name(&format!("low pass filter {} {}", cutoff, transition_bw));
    let low_pass_filter = FirBuilder::new_resampling::<Complex<f32>, Complex<f32>>(
        audio_rate as usize,
        file_rate as usize,
    );

    const VOLUME_ADJUSTEMENT: f64 = 0.5;
    const MID_AUDIO_SPECTRUM_FREQ: u32 = 1500;
    let mut ssb_lo_index: u32 = 0;
    let mut weaver_ssb_decode = Apply::new(move |v: &Complex<f32>| {
        let local_oscillator_phase = 2.0f64
            * std::f64::consts::PI
            * (ssb_lo_index as f64)
            * (MID_AUDIO_SPECTRUM_FREQ as f64)
            / (audio_rate as f64);
        let term1 = v.re as f64 * local_oscillator_phase.cos();
        let term2 = v.im as f64 * local_oscillator_phase.sin();
        let result = VOLUME_ADJUSTEMENT * (term1 + term2); // substraction for LSB, addition for USB
        ssb_lo_index = (ssb_lo_index + 1) % audio_rate;
        result as f32
    });
    weaver_ssb_decode.set_instance_name("Weaver SSB decoder");

    // let zmq_snk = PubSinkBuilder::new(8)
    //         .address("tcp://127.0.0.1:50001")
    //         .build();

    let snk = AudioSink::new(audio_rate, 1);

    let src = fg.add_block(src);
    let freq_xlating = fg.add_block(freq_xlating);
    let low_pass_filter = fg.add_block(low_pass_filter);
    let weaver_ssb_decode = fg.add_block(weaver_ssb_decode);
    let snk = fg.add_block(snk);
    // let zmq_snk = fg.add_block(zmq_snk);

    const SLAB_SIZE: usize = 2 * 2 * 8192;
    fg.connect_stream_with_type(src, "out", freq_xlating, "in", Slab::with_size(SLAB_SIZE))?;
    fg.connect_stream(freq_xlating, "out", low_pass_filter, "in")?;
    fg.connect_stream(low_pass_filter, "out", weaver_ssb_decode, "in")?;
    // fg.connect_stream(low_pass_filter, "out", zmq_snk, "in")?;
    fg.connect_stream(weaver_ssb_decode, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
