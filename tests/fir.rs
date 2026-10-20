use anyhow::Result;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use num_complex::Complex32;

#[test]
fn fir_f32() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    let taps = [1.0f32, 1.0, 1.0];

    let src = VectorSource::<f32>::new(orig);
    let fir = FirBuilder::fir::<f32, f32, _>(taps);
    let snk = VectorSink::<f32>::new(6);

    connect!(fg, src > fir > snk);

    let fg = Runtime::new().run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    let res = vec![6.0f32, 9.0, 12.0, 15.0];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert!((have - want).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fir_complex_batches_and_tail() -> Result<()> {
    let mut fg = Flowgraph::new();
    let input: Vec<_> = (0..2053)
        .map(|k| Complex32::new((k % 101) as f32 / 101.0, (k % 79) as f32 / 79.0))
        .collect();
    let taps: [f32; 127] = std::array::from_fn(|k| (k % 37) as f32 / 127.0);
    let expected: Vec<Complex32> = input
        .windows(taps.len())
        .map(|window| {
            window
                .iter()
                .zip(taps.iter().rev())
                .map(|(s, t)| s * t)
                .sum()
        })
        .collect();

    let src = VectorSource::<Complex32>::new(input);
    let fir = FirBuilder::fir::<Complex32, Complex32, _>(taps);
    let snk = VectorSink::<Complex32>::new(expected.len());
    connect!(fg, src > fir > snk);
    let fg = Runtime::new().run(fg)?;
    let snk = fg.block(&snk)?;
    assert_eq!(snk.items().len(), expected.len());
    for (have, want) in snk.items().iter().zip(expected) {
        assert!((have - want).norm() < 1e-5);
    }
    Ok(())
}
