use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let freq = 93.1 * 1e6; // change to set frequency
    let sample_rate = 250_000.0;

    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(freq)
        .sample_rate(sample_rate)
        .gain(34.0)
        .build();

    let snk = AudioSink::new(sample_rate as u32, 1);

    let mut last = Complex32::new(0.0, 0.0);
    let demod = Apply::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg();
        last = *v;
        arg
    });

    let src = fg.add_block(src);
    let demod = fg.add_block(demod);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", demod, "in")?;
    fg.connect_stream(demod, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
