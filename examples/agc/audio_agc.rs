use futuresdr::anyhow::Result;
use futuresdr::async_io;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::{AgcBuilder, Combine, SignalSourceBuilder};
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    // Generate 220Hz tone
    let src = SignalSourceBuilder::<f32>::sin(220.0, 48_000.0)
        .amplitude(0.4)
        .build();
    // Modulation Wave for the 220Hz tone, changing from loud to silent every second
    let gain_change = SignalSourceBuilder::<f32>::sin(0.5, 48_000.0)
        .amplitude(0.5)
        .build();
    // Modulate Tone with the modulation wave
    let combine = Combine::new(|a: &f32, b: &f32| a * b);
    // Set the Automatic Gain Control settings
    let agc = AgcBuilder::<f32>::new()
        .squelch(0.0)
        .max_gain(65536.0)
        .adjustment_rate(0.1)
        .reference_power(1.0)
        .build();
    let gain_locked_handler_id = agc.message_input_name_to_id("gain_locked").unwrap();
    let max_gain_handler_id = agc.message_input_name_to_id("max_gain").unwrap();
    let _adjustment_rate_handler_id = agc.message_input_name_to_id("adjustment_rate").unwrap();
    let reference_power_handler_id = agc.message_input_name_to_id("reference_power").unwrap();

    // Audiosink to output the modulated tone
    let audio_snk = AudioSink::new(48_000, 1);

    connect!(fg,
             src > combine.in0;
             gain_change.out > combine.in1;
             combine > agc > audio_snk;
    );

    // Start the flowgraph and save the handle
    let (_res, mut handle) = async_io::block_on(Runtime::new().start(fg));

    // Keep changing gain and gain lock.
    loop {
        // Reference power of 1.0 is the power level we want to achieve
        println!("Setting reference power to 1.0");
        async_io::block_on(handle.call(agc, reference_power_handler_id, Pmt::F32(1.0)))?;

        // A high max gain allows to amplify a signal stronger
        println!("Setting Max Gain to 65536.0");
        async_io::block_on(handle.call(agc, max_gain_handler_id, Pmt::F32(65536.0)))?;
        sleep(Duration::from_secs(5));

        // Setting a gain lock prevents gain changes from happening
        println!("Setting gain lock for 5s");
        async_io::block_on(handle.call(agc, gain_locked_handler_id, Pmt::Bool(true)))?;

        // Audio should get quiet faster, but gain is still locked here. it will be released after 5 seconds
        println!("Setting reference power to 0.2");
        async_io::block_on(handle.call(agc, reference_power_handler_id, Pmt::F32(0.2)))?;
        sleep(Duration::from_secs(5));

        // Gain lock released! Audio should get more quiet here for 10 seconds
        println!("Releasing gain lock");
        async_io::block_on(handle.call(agc, gain_locked_handler_id, Pmt::Bool(false)))?;
        sleep(Duration::from_secs(10));
    }
}
