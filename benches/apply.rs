use criterion::criterion_group;
use criterion::criterion_main;
use criterion::Criterion;
use std::iter::repeat_with;

use futuresdr::blocks::Apply;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

pub fn apply(c: &mut Criterion) {
    let n_samp = 123456;
    let input: Vec<u32> = repeat_with(rand::random::<u32>).take(n_samp).collect();

    let mut group = c.benchmark_group("apply");

    group.throughput(criterion::Throughput::Elements(n_samp as u64));

    group.bench_function(format!("mock-u32-plus-1-{n_samp}"), |b| {
        b.iter(|| {
            let block: Apply<_, _, _, Reader<u32>, Writer<u32>> = Apply::new(|x: &u32| x + 1);

            let mut mocker = Mocker::new(block);
            mocker.input().set(input.clone());
            mocker.output().reserve(n_samp);
            mocker.run();
        });
    });

    group.finish();
}

criterion_group!(benches, apply);
criterion_main!(benches);
