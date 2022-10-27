use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;
use std::time::Instant;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn run_fg(n_samp: u64) -> Result<()> {
    let mut fg = Flowgraph::new();

    let null_source = fg.add_block(NullSource::<f32>::new());
    let head = fg.add_block(Head::<f32>::new(n_samp));
    let copy = fg.add_block(Copy::<f32>::new());
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(null_source, "out", head, "in")?;
    fg.connect_stream(head, "out", copy, "in")?;
    fg.connect_stream(copy, "out", vect_sink, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

fn run_fg_timed(n_samp: u64, iters: u64) -> Result<Duration> {
    let mut duration = Duration::from_secs(0);
    for _ in 0..iters {
        let mut fg = Flowgraph::new();

        let null_source = fg.add_block(NullSource::<f32>::new());
        let head = fg.add_block(Head::<f32>::new(n_samp));
        let copy = fg.add_block(Copy::<f32>::new());
        let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

        fg.connect_stream(null_source, "out", head, "in")?;
        fg.connect_stream(head, "out", copy, "in")?;
        fg.connect_stream(copy, "out", vect_sink, "in")?;

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
