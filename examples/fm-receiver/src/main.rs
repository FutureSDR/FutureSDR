//! A simple FM receiver that you can tune to nearby radio stations
//!
//! When you run the example, it will build a flowgraph consisting of the following blocks:
//! * SoapySource: Gets data from your SDR using the SoapySDR driver
//! * Demodulator: Demodulates the FM signal
//! * AudioSink: Plays the demodulated signal on your device
//!
//! After giving it some time to start up the SDR, it enters a loop where you will
//! be periodically asked to enter a new frequency that the SDR will be tuned to.
//! **Watch out** though: Some frequencies (very high or very low) might be unsupported
//! by your SDR and may cause a crash.
use clap::Parser;

use futuredsp::firdes;
use futuresdr::async_io;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

#[derive(Parser, Debug)]
struct Args {
    /// Gain to apply to the soapy source
    #[clap(short, long, default_value_t = 30.0)]
    gain: f64,

    /// Center frequency
    #[clap(short, long, default_value_t = 100_000_000.0)]
    frequency: f64,

    /// Sample rate
    #[clap(short, long, default_value_t = 1000000.0)]
    rate: f64,

    /// Soapy source to use as a source
    #[clap(long, default_value = "")]
    soapy: String,
}

fn main() -> ! {
    let args = Args::parse();

    let sample_rate = args.rate as u32;
    let audio_rate = AudioSink::default_sample_rate().unwrap();

    println!("Configuration {:?}", args);
    println!("Audio Rate {:?}", audio_rate);

    // Create the `Flowgraph` where the `Block`s will be added later on
    let mut fg = Flowgraph::new();

    // Create a new SoapySDR block with the given parameters
    let src = SoapySourceBuilder::new()
        .filter(args.soapy)
        .freq(args.frequency)
        .sample_rate(args.rate)
        .gain(args.gain)
        .build();

    // Store the `freq` port ID for later use
    let freq_port_id = src
        .message_input_name_to_id("freq")
        .expect("No freq port found!");

    // Downsample before demodulation
    let interp = (audio_rate * 5) as usize;
    let decim = sample_rate as usize;
    println!("interp {}   decim {}", interp, decim);
    let resamp1 = FirBuilder::new_resampling::<Complex32>(interp, decim);

    // Demodulation block using the conjugate delay method
    // See https://en.wikipedia.org/wiki/Detector_(radio)#Quadrature_detector
    let mut last = Complex32::new(0.0, 0.0); // store sample x[n-1]
    let demod = Apply::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg(); // Obtain phase of x[n] * conj(x[n-1])
        last = *v;
        arg
    });

    // Design filter for the audio and decimate by 5.
    // Ideally, this should be a FM de-emphasis filter, but the following works.
    let cutoff = 2_000.0 / (audio_rate * 5) as f64;
    let transition = 10_000.0 / (audio_rate * 5) as f64;
    println!("cutoff {}   transition {}", cutoff, transition);
    let audio_filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition, 0.1);
    let resamp2 = FirBuilder::new_resampling_with_taps::<f32, f32, _>(1, 5, audio_filter_taps);

    // Single-channel `AudioSink` with the downsampled rate (sample_rate / (8*5) = 48_000)
    let snk = AudioSink::new(audio_rate, 1);
    // Add all the blocks to the `Flowgraph`...
    let src = fg.add_block(src);
    let resamp1 = fg.add_block(resamp1);
    let demod = fg.add_block(demod);
    let resamp2 = fg.add_block(resamp2);
    let snk = fg.add_block(snk);

    // ... and connect the ports appropriately
    fg.connect_stream(src, "out", resamp1, "in").unwrap();
    fg.connect_stream(resamp1, "out", demod, "in").unwrap();
    fg.connect_stream(demod, "out", resamp2, "in").unwrap();
    fg.connect_stream(resamp2, "out", snk, "in").unwrap();

    // Start the flowgraph and save the handle
    let (_res, mut handle) = Runtime::new().start(fg);

    // Keep asking user for a new frequency and a new sample rate
    loop {
        println!("Please enter a new frequency");
        // Get input from stdin and remove all whitespace (most importantly '\n' at the end)
        let mut input = String::new(); // Input buffer
        std::io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");
        input.retain(|c| !c.is_whitespace());

        // If the user entered a valid number, set the new frequency by sending a message to the `FlowgraphHandle`
        if let Ok(new_freq) = input.parse::<u32>() {
            println!("Setting frequency to {}", input);
            async_io::block_on(handle.call(src, freq_port_id, Pmt::U32(new_freq))).unwrap();
        } else {
            println!("Input not parsable to u32: {}", input);
        }
    }
}
