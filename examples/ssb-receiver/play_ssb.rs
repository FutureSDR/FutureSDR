use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::Throttle;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuredsp::{firdes, windows};
use futuresdr::blocks::Apply;
use futuresdr::blocks::ApplyNM;
use num_complex::Complex;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::blocks::zeromq::PubSinkBuilder;

// Inspired by https://wiki.gnuradio.org/index.php/Simulation_example:_Single_Sideband_transceiver 

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    const FILE_SAMPLING_RATE: u32 = 256_000;
    const CENTER_FREQ: u32 = 51_500 - 6960; // explanation in http://www.csun.edu/~skatz/katzpage/sdr_project/sdr/grc_tutorial4.pdf

    // To be downloaded from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip
    let src = FileSource::<Complex<f32>>::repeat("ssb_lsb_256k_complex2.dat");

    const FILE_LEVEL_ADJUSTEMENT: f32 = 0.0001;
    let mut xlating_local_oscillator_index: u32 = 0;
    let freq_xlating = Apply::<Complex<f32>, Complex<f32>>::new(move |v| {
        let local_oscillator_phase =  2.0f64 * std::f64::consts::PI * (xlating_local_oscillator_index as f64) * (CENTER_FREQ as f64) / (FILE_SAMPLING_RATE as f64);
        let lo_v = Complex::<f32>::new(local_oscillator_phase.cos() as f32,  local_oscillator_phase.sin() as f32);
        xlating_local_oscillator_index = (xlating_local_oscillator_index + 1) % FILE_SAMPLING_RATE;
        let result = FILE_LEVEL_ADJUSTEMENT * v * lo_v;
        result
    });

    const AUDIO_SAMPLING_RATE: u32 = 32_000;
    const DOWNSAMPLING: usize = (FILE_SAMPLING_RATE / AUDIO_SAMPLING_RATE) as usize;
    let downsampler = ApplyNM::<Complex<f32>, Complex<f32>, DOWNSAMPLING, 1>::new(
        move |v: &[Complex<f32>], d: &mut [Complex<f32>]| {
            d[0] = v[0];
        },
    );

    let cutoff = 2_000.0f64 / AUDIO_SAMPLING_RATE as f64;
    let transition_bw = 100.0f64 / AUDIO_SAMPLING_RATE as f64;
    let max_ripple = 0.01;
    let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    println!("Filter has {} taps", taps.len());
    let low_pass_filter = FirBuilder::new::<Complex<f32>, f32, _>(taps);



    const VOLUME_ADJUSTEMENT: f64 = 0.5;
    const MID_AUDIO_SPECTRUM_FREQ: u32 = 1500;
    let mut ssb_lo_index: u32 = 0;
    let weaver_ssb_decode = Apply::<Complex<f32>, f32>::new(move |v| {
        let local_oscillator_phase =  2.0f64 * std::f64::consts::PI * (ssb_lo_index as f64) * (MID_AUDIO_SPECTRUM_FREQ as f64) / (AUDIO_SAMPLING_RATE as f64);
        let term1 = v.re as f64 * local_oscillator_phase.cos();
        let term2 = v.im as f64 * local_oscillator_phase.sin();
        let result = VOLUME_ADJUSTEMENT * (term1 - term2); // substraction for LSB, addition for USB
        ssb_lo_index = (ssb_lo_index + 1) % AUDIO_SAMPLING_RATE;
        result as f32
    });

    let debug_throttle = Throttle::<Complex<f32>>::new(AUDIO_SAMPLING_RATE as f64);
    let zmq_snk = PubSinkBuilder::new(8)
            .address("tcp://127.0.0.1:50001")
            .build();

    let snk = AudioSink::new(AUDIO_SAMPLING_RATE, 1);

    let src = fg.add_block(src);
    let freq_xlating = fg.add_block(freq_xlating);
    let low_pass_filter = fg.add_block(low_pass_filter);
    let downsampler = fg.add_block(downsampler);
    let weaver_ssb_decode = fg.add_block(weaver_ssb_decode);
    let snk = fg.add_block(snk);
   // let debug_throttle = fg.add_block(debug_throttle);
    let zmq_snk = fg.add_block(zmq_snk);

    const SLAB_SIZE: usize = 2*2*8192;
    fg.connect_stream_with_type(src, "out", freq_xlating, "in", Slab::with_size(SLAB_SIZE))?;
    fg.connect_stream_with_type(freq_xlating, "out", downsampler, "in", Slab::with_size(SLAB_SIZE))?;
    fg.connect_stream(downsampler, "out", low_pass_filter, "in")?;
    //fg.connect_stream_with_type(downsampler, "out", weaver_ssb_decode, "in", Slab::with_size(SLAB_SIZE))?;
    fg.connect_stream(low_pass_filter, "out", weaver_ssb_decode, "in")?;
    fg.connect_stream(weaver_ssb_decode, "out", snk, "in")?;
   // fg.connect_stream(downsampler, "out", debug_throttle, "in")?;
    fg.connect_stream(low_pass_filter, "out", zmq_snk, "in")?;
    Runtime::new().run(fg)?;

    Ok(())
}
