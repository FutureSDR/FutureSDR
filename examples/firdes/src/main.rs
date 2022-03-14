use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Source;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use futuredsp::firdes;
use futuresdr::anyhow::Result;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    const SAMPLING_FREQ: u32 = 44_100;
    const TONE_FREQ: (f32, f32, f32) = (2000.0, 6000.0, 10000.0);
    let enable_filter = true;

    static mut T: usize = 0;
    let src = Source::<f32>::new(|| {
        let x = unsafe {
            T += 1;
            T
        };
        let freq = match (x as f32 % SAMPLING_FREQ as f32) as u32 {
            x if x < SAMPLING_FREQ / 3 => TONE_FREQ.0,
            x if x < 2 * SAMPLING_FREQ / 3 => TONE_FREQ.1,
            _ => TONE_FREQ.2,
        };
        (2.0 * std::f32::consts::PI * x as f32 * freq / SAMPLING_FREQ as f32).sin()
    });

    // Design bandpass filter for the middle tone
    let lower_cutoff = (TONE_FREQ.1 - 200.0) / SAMPLING_FREQ as f32;
    let higher_cutoff = (TONE_FREQ.1 + 200.0) / SAMPLING_FREQ as f32;
    let transition_bw = 500.0 / SAMPLING_FREQ as f32;
    let max_ripple = 0.01;

    let filter_taps =
        firdes::kaiser::bandpass(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
    println!("Filter has {} taps", filter_taps.len());

    let filter_block = match enable_filter {
        true => FirBuilder::new::<f32, f32, _>(filter_taps),
        _ => FirBuilder::new::<f32, f32, _>([1.0_f32]),
    };

    let snk = AudioSink::new(SAMPLING_FREQ, 1);

    let src = fg.add_block(src);
    let filter_block = fg.add_block(filter_block);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", filter_block, "in")?;
    fg.connect_stream(filter_block, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
