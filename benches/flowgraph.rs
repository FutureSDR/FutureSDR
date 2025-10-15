use anyhow::Result;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::prelude::*;
use std::hint::black_box;
use std::time::Duration;
use std::time::Instant;

fn run_fg(n_samp: u64) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let head = Head::<f32>::new(n_samp);
    let copy = Copy::<f32>::new();
    let snk = VectorSink::<f32>::new(n_samp as usize);

    connect!(fg, src > head > copy > snk);

    Runtime::new().run(fg)?;
    Ok(())
}

fn run_fg_timed(n_samp: u64, iters: u64) -> Result<Duration> {
    let mut duration = Duration::from_secs(0);
    for _ in 0..iters {
        let mut fg = Flowgraph::new();

        let src = NullSource::<f32>::new();
        let head = Head::<f32>::new(n_samp);
        let copy = Copy::<f32>::new();
        let snk = VectorSink::<f32>::new(n_samp as usize);

        connect!(fg, src > head > copy > snk);

        let now = Instant::now();
        Runtime::new().run(fg)?;
        duration += now.elapsed();
    }

    Ok(duration)
}

pub fn flowgraph(c: &mut Criterion) {
    let n_samp = 123456;

    let mut group = c.benchmark_group("flowgraph");

    group.throughput(criterion::Throughput::Elements(n_samp));

    group.bench_function(format!("overall-{n_samp}"), |b| {
        b.iter(|| {
            run_fg(black_box(n_samp)).unwrap();
        });
    });

    group.bench_function(format!("run-{n_samp}"), |b| {
        b.iter_custom(|iters: u64| run_fg_timed(black_box(n_samp), black_box(iters)).unwrap());
    });

    group.finish();
}

criterion_group!(benches, flowgraph);
criterion_main!(benches);
