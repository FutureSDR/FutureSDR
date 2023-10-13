#![allow(clippy::excessive_precision)]
use codec2::Codec2;
use codec2::Codec2Mode;
use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::FiniteSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::fs::File;
use std::path::Path;

use m17::CallSign;
use m17::EncoderBlock;
use m17::LinkSetupFrame;
use m17::RRC_TAPS;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let mut in_file = File::open(Path::new("rick.wav"))?;
    let (header, data) = wav::read(&mut in_file)?;
    assert_eq!(header.channel_count, 1);
    assert_eq!(header.sampling_rate, 8000);
    assert_eq!(header.audio_format, wav::WAV_FORMAT_PCM);
    assert_eq!(header.bits_per_sample, 16);
    let data = data.try_into_sixteen().unwrap();

    let mut i = 0;
    let src = FiniteSource::new(move || {
        if i >= data.len() {
            None
        } else {
            i += 1;
            Some(data[i - 1])
        }
    });

    let mut c2 = Codec2::new(Codec2Mode::MODE_3200);
    assert_eq!(c2.samples_per_frame(), 160);
    assert_eq!(c2.bits_per_frame(), 64);

    let codec2 = ApplyNM::<_, _, _, 160, { (64 + 7) / 8 }>::new(move |i: &[i16], o: &mut [u8]| {
        c2.encode(o, i);
    });

    let lsf = LinkSetupFrame::new(CallSign::new_id("DF1BBL"), CallSign::new_broadcast());
    let encoder = EncoderBlock::new(lsf);
    let pulse = ApplyNM::<_, _, _, 1, 10>::new(move |i: &[f32], o: &mut [f32]| {
        o.fill(0.0);
        o[0] = i[0];
    });

    let rrc = FirBuilder::new::<f32, f32, f32, _>(RRC_TAPS);

    let mut curr = Complex32::new(0.8, 0.0);
    let sensitivity = 2.0 * std::f32::consts::PI * 800.0 / 48000.0;
    let fm = Apply::new(move |i: &f32| {
        let c = Complex32::from_polar(1.0, i * 3.3 * sensitivity);
        curr *= c;
        curr
    });
    let snk = FileSink::<Complex32>::new("input.cf32");
    connect!(fg, src > codec2 > encoder > pulse > rrc > fm > snk);

    // let upsample = FirBuilder::new_resampling::<Complex32, Complex32>(16, 1);
    //
    // let snk = SinkBuilder::new()
    //     .frequency(433475000.0 * (1.0 + 2.75e-6))
    //     .gain(60.0)
    //     .sample_rate(16.0 * 48000.0)
    //     .build()?;
    //
    // connect!(fg, src > codec2 > encoder > pulse > rrc > fm > upsample > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
