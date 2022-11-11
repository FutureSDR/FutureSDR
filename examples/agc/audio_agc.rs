use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::{AGC, Combine, FirBuilder, SignalSourceBuilder};
use futuresdr::blocks::zeromq::PubSinkBuilder;
use futuresdr::blocks::ApplyNM;
use futuresdr::log::info;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::macros::connect;

use futuredsp::firdes;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    // Design bandpass filter for the middle tone
    let cutoff = (440.0) as f64 / 44_100. as f64;
    let transition_bw = 100.0 / 44_100. as f64;
    let max_ripple = 0.01;

    let filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    info!("Filter has {} taps", filter_taps.len());

    let src = SignalSourceBuilder::<f32>::sin(220.0, 44100.0)
        .amplitude(0.4)
        .build();
    //let src = Oscillator::new(440.0, 1.0, 44100.0);
    let gain_change = SignalSourceBuilder::<f32>::sin(0.5, 44100.0)
        .amplitude(0.5)
        .build();
    //let gain_change = Oscillator::new(0.5, 1.5, 44100.0);
    let combine = Combine::new(|a: &f32, b: &f32| {
        a * b
    });

    let agc = AGC::<f32>::new(0.0, 1.0);

    let lowpass = FirBuilder::new::<f32, f32, _, _>(filter_taps);

    //let throttle = Throttle::<f32>::new(44_100.);
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0];
        d[1] = v[0];
    });
    let audio_snk = AudioSink::new(44_100, 2);
    let zmq_snk = PubSinkBuilder::<f32>::new()
        .address("tcp://127.0.0.1:50001")
        .build();

    connect!(fg,
             src > combine.in0;
             gain_change.out > combine.in1;
             combine > agc;
             agc > lowpass;
             lowpass > zmq_snk;
             lowpass > mono_to_stereo;
             mono_to_stereo > audio_snk;
    );

    Runtime::new().run(fg)?;

    Ok(())
}
