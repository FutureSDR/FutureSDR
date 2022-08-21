use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Source;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use futuredsp::firdes;
use futuresdr::anyhow::Result;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    const SAMPLING_FREQ: u32 = 66_150;
    const TONE_FREQ: (f32, f32, f32) = (2000.0, 6000.0, 10000.0);
    let enable_filter = true;

    let mut t: usize = 0;
    let src = Source::new(move || {
        t += 1;
        let freq = match (t as f32 % SAMPLING_FREQ as f32) as u32 {
            x if x < SAMPLING_FREQ / 3 => TONE_FREQ.0,
            x if x < 2 * SAMPLING_FREQ / 3 => TONE_FREQ.1,
            _ => TONE_FREQ.2,
        };
        (2.0 * std::f32::consts::PI * t as f32 * freq / SAMPLING_FREQ as f32).sin()
    });

    // Resample to 44.100 kHz (downsample by a factor of 2/3)
    let interp = 2;
    let decim = 3;
    const DOWNSAMPLED_FREQ: u32 = 44_100;
    let resamp_block = FirBuilder::new_resampling::<f32, f32>(interp, decim);

    // Design bandpass filter for the middle tone
    let lower_cutoff = (TONE_FREQ.1 - 200.0) as f64 / DOWNSAMPLED_FREQ as f64;
    let higher_cutoff = (TONE_FREQ.1 + 200.0) as f64 / DOWNSAMPLED_FREQ as f64;
    let transition_bw = 500.0 / DOWNSAMPLED_FREQ as f64;
    let max_ripple = 0.01;

    let filter_taps =
        firdes::kaiser::bandpass::<f32>(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
    println!("Filter has {} taps", filter_taps.len());

    let filter_block = match enable_filter {
        true => FirBuilder::new::<f32, f32, _, _>(filter_taps),
        _ => FirBuilder::new::<f32, f32, _, _>([1.0_f32]),
    };

    let snk = AudioSink::new(DOWNSAMPLED_FREQ, 1);

    let src = fg.add_block(src);
    let resamp_block = fg.add_block(resamp_block);
    let filter_block = fg.add_block(filter_block);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", resamp_block, "in")?;
    fg.connect_stream(resamp_block, "out", filter_block, "in")?;
    fg.connect_stream(filter_block, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
