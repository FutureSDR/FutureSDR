use std::thread::sleep;
use std::time::Duration;
use futuresdr::async_io;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::{AGCBuilder, Combine, FirBuilder, SignalSourceBuilder};
use futuresdr::blocks::ApplyNM;
use futuresdr::log::info;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::Pmt;
use futuresdr::macros::connect;
use futuredsp::firdes;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    // Design bandpass filter for the middle tone
    let cutoff = 440.0 / 48_000.0;
    let transition_bw = 100.0 / 48_000.0;
    let max_ripple = 0.01;

    let filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    info!("Filter has {} taps", filter_taps.len());

    // Generate 220Hz tone
    let src = SignalSourceBuilder::<f32>::sin(220.0, 48_000.0)
        .amplitude(0.4)
        .build();
    // Modulation Wave for the 220Hz tone, changing from loud to silent every second
    let gain_change = SignalSourceBuilder::<f32>::sin(0.5, 48_000.0)
        .amplitude(0.5)
        .build();
    // Modulate Tone with the modulation wave
    let combine = Combine::new(|a: &f32, b: &f32| {
        a * b
    });
    // Set the Automatic Gain Control settings
    let agc = AGCBuilder::new()
        .squelch(0.0)
        .max_gain(65536.0)
        .adjustment_rate(0.1)
        .reference_power(1.0)
        .build();
    let gain_lock_handler_id = agc.message_input_name_to_id("gain_lock").unwrap();
    let max_gain_handler_id = agc.message_input_name_to_id("max_gain").unwrap();
    let _adjustment_rate_handler_id = agc.message_input_name_to_id("adjustment_rate").unwrap();
    let reference_power_handler_id = agc.message_input_name_to_id("reference_power").unwrap();

    // Lowpass filter to smoothen the waveform.
    let lowpass = FirBuilder::new::<f32, f32, _, _>(filter_taps);

    // Converting to stereo. Might not be necessary on your system
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0];
        d[1] = v[0];
    });
    // Audiosink to output the modulated tone
    let audio_snk = AudioSink::new(48_000, 2);

    connect!(fg,
             src > combine.in0;
             gain_change.out > combine.in1;
             combine > agc > lowpass;
             lowpass > mono_to_stereo > audio_snk;
    );

    // Start the flowgraph and save the handle
    let (_res, mut handle) = async_io::block_on(Runtime::new().start(fg));

    // Keep changing gain and gain lock.
    loop {
        // Reference power of 1.0 is the power level we want to achieve
        println!("Setting reference power to 1.0");
        async_io::block_on(handle.call(
            agc,
            reference_power_handler_id,
            Pmt::F32(1.0),
        ))?;

        // A high max gain allows to amplify a signal stronger
        println!("Setting Max Gain to 65536.0");
        async_io::block_on(handle.call(
            agc,
            max_gain_handler_id,
            Pmt::F32(65536.0),
        ))?;
        sleep(Duration::from_secs(5));

        // Setting a gain lock prevents gain changes from happening
        println!("Setting gain lock for 5s");
        async_io::block_on(handle.call(
            agc,
            gain_lock_handler_id,
            Pmt::U32(1),
        ))?;

        // Audio should get quiet faster, but gain is still locked here. it will be released after 5 seconds
        println!("Setting reference power to 0.2");
        async_io::block_on(handle.call(
            agc,
            reference_power_handler_id,
            Pmt::F32(0.2),
        ))?;
        sleep(Duration::from_secs(5));

        // Gain lock released! Audio should get more quiet here for 10 seconds
        println!("Releasing gain lock");
        async_io::block_on(handle.call(
            agc,
            gain_lock_handler_id,
            Pmt::U32(0),
        ))?;
        sleep(Duration::from_secs(10));

    }
}
