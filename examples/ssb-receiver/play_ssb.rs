use futuredsp::firdes;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
// use futuresdr::blocks::zeromq::PubSinkBuilder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use num_complex::Complex;

// Inspired by https://wiki.gnuradio.org/index.php/Simulation_example:_Single_Sideband_transceiver

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    const FILE_SAMPLING_RATE: u32 = 256_000;
    const CENTER_FREQ: i32 = 51_500; // explanation in http://www.csun.edu/~skatz/katzpage/sdr_project/sdr/grc_tutorial4.pdf

    // To be downloaded from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip
    let file_name = "ssb_lsb_256k_complex2.dat";
    let src_name = format!("File {}", file_name);
    let mut src = FileSource::<Complex<f32>>::repeat(file_name);
    src.set_instance_name(&src_name);

    const FILE_LEVEL_ADJUSTEMENT: f32 = 0.0001;
    let mut xlating_local_oscillator_index: u32 = 0;
    const FWT0: f32 =
        -2.0 * std::f32::consts::PI * (CENTER_FREQ as f32) / (FILE_SAMPLING_RATE as f32);
    let mut freq_xlating = Apply::new(move |v: &Complex<f32>| {
        let lo_v = Complex::<f32>::new(0.0, (xlating_local_oscillator_index as f32) * FWT0).exp();
        xlating_local_oscillator_index = (xlating_local_oscillator_index + 1) % FILE_SAMPLING_RATE;
        let result = FILE_LEVEL_ADJUSTEMENT * v * lo_v;
        result
    });
    freq_xlating.set_instance_name(&format!("freq_xlating {}", CENTER_FREQ));

    const AUDIO_SAMPLING_RATE: u32 = 32_000;
    const DOWNSAMPLING: usize = (FILE_SAMPLING_RATE / AUDIO_SAMPLING_RATE) as usize;

    let cutoff = 3_000.0f64 / AUDIO_SAMPLING_RATE as f64;
    let transition_bw = 100.0f64 / AUDIO_SAMPLING_RATE as f64;
    let max_ripple = 0.01;
    let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    println!("Filter has {} taps", taps.len());
    let mut low_pass_filter = FirBuilder::new_resampling_with_taps::<
        Complex<f32>,
        Complex<f32>,
        f32,
        _,
    >(1, DOWNSAMPLING, taps);

    low_pass_filter.set_instance_name(&format!("low pass filter {} {}", cutoff, transition_bw));

    const VOLUME_ADJUSTEMENT: f64 = 0.5;
    const MID_AUDIO_SPECTRUM_FREQ: u32 = 1500;
    let mut ssb_lo_index: u32 = 0;
    let mut weaver_ssb_decode = Apply::new(move |v: &Complex<f32>| {
        let local_oscillator_phase = 2.0f64
            * std::f64::consts::PI
            * (ssb_lo_index as f64)
            * (MID_AUDIO_SPECTRUM_FREQ as f64)
            / (AUDIO_SAMPLING_RATE as f64);
        let term1 = v.re as f64 * local_oscillator_phase.cos();
        let term2 = v.im as f64 * local_oscillator_phase.sin();
        let result = VOLUME_ADJUSTEMENT * (term1 + term2); // substraction for LSB, addition for USB
        ssb_lo_index = (ssb_lo_index + 1) % AUDIO_SAMPLING_RATE;
        result as f32
    });
    weaver_ssb_decode.set_instance_name("Weaver SSB decoder");

    // let zmq_snk = PubSinkBuilder::new(8)
    //         .address("tcp://127.0.0.1:50001")
    //         .build();

    let snk = AudioSink::new(AUDIO_SAMPLING_RATE, 1);

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
