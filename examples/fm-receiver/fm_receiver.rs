use anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Apply2;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::FileSink;
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
    //let snk = FileSink::new(1, "/tmp/futuresdr-snk.wav");
    //let mag = Apply::new(|c: &Complex<f32>| -> f32 { ((*c).re * (*c).re + (*c).im * (*c).im).sqrt() });
    let mag = Apply2::new(|v_n_minus_1: &Complex<f32>, v_n: &Complex<f32>| -> f32 { 
        (v_n.re * v_n_minus_1.im - v_n.im * v_n_minus_1.re) / (v_n.re * v_n.re + v_n.im * v_n.im)
    });

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);
    let mag = fg.add_block(mag);

    fg.connect_stream(src, "out", mag, "in")?;
    fg.connect_stream(mag, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
