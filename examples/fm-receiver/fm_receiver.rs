use anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use num_complex::Complex;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let sample_rate = 250_000.0;
    let src = SoapySourceBuilder::new()
        .filter("driver=rtlsdr".to_string())
        .freq(97_300_000.0)
        .sample_rate(sample_rate)
        .gain(34.0)
        .build();

    let snk = AudioSink::new(sample_rate as u32, 1);
    let mut v_n_minus_1: Complex<f32> = Complex {
        re: (0.0f32),
        im: (0.0f32),
    };
    let mag = Apply::new(move |v_n: &Complex<f32>| -> f32 {
        let r = (v_n.re * v_n_minus_1.im - v_n.im * v_n_minus_1.re)
            / (v_n.re * v_n.re + v_n.im * v_n.im);
        v_n_minus_1 = *v_n;
        return r;
    });

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);
    let mag = fg.add_block(mag);

    fg.connect_stream(src, "out", mag, "in")?;
    fg.connect_stream(mag, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
