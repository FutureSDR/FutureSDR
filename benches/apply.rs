use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;

use futuresdr::blocks::Apply;
use futuresdr::runtime::Mocker;

pub fn apply(c: &mut Criterion) {
    let n_samp = 123456;
    let input: Vec<u32> = rand::thread_rng()
        .sample_iter(rand::distributions::Uniform::<u32>::new(0, 1024))
        .take(n_samp)
        .collect();

    let mut group = c.benchmark_group("apply");

    group.throughput(criterion::Throughput::Elements(n_samp as u64));

    group.bench_function(format!("mock-u32-plus-1-{n_samp}"), |b| {
        b.iter(|| {
            let block = Apply::new(|x: &u32| x + 1);

            let mut mocker = Mocker::new(block);
            mocker.input(0, input.clone());
            mocker.init_output::<u32>(0, n_samp);
            mocker.run();
        });
    });

    group.finish();
}

criterion_group!(benches, apply);
criterion_main!(benches);
