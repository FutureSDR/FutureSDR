use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use std::hint::black_box;

use futuresdr::prelude::*;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

const N: usize = 1024 * 1024 * 64;

#[derive(Block)]
struct AsyncTest {
    #[input]
    input: Reader<u8>,
    #[output]
    output: Writer<u8>,
}

impl AsyncTest {
    fn sync_work(
        &mut self,
        _io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i_len = self.input.slice().len();
        let o_len = self.output.slice().len();

        self.input.consume(i_len);
        self.output.produce(o_len);

        Ok(())
    }
}

impl Kernel for AsyncTest {
    fn work(
        &mut self,
        io: &mut WorkIo,
        m: &mut MessageOutputs,
        b: &mut BlockMeta,
    ) -> impl std::future::Future<Output = Result<()>> {
        std::future::ready(black_box(self.sync_work(io, m, b)))
    }
}

pub fn sync_vs_async(c: &mut Criterion) {
    let n_samp = 0;
    let input = Vec::new();

    let mut group = c.benchmark_group("sync-vs-async");

    group.bench_function("sync".to_string(), |b| {
        let block: AsyncTest = AsyncTest {
            input: Default::default(),
            output: Default::default(),
        };
        let mut mocker = Mocker::new(block);
        mocker.input().set(input.clone());
        mocker.output().reserve(n_samp);
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        b.iter(move || {
            async_io::block_on(async {
                for _ in 0..N {
                    let _ = black_box(mocker.block.kernel.sync_work(
                        &mut io,
                        &mut mocker.block.mio,
                        &mut mocker.block.meta,
                    ));
                }
            })
        });
    });

    group.bench_function("async".to_string(), |b| {
        let block: AsyncTest = AsyncTest {
            input: Default::default(),
            output: Default::default(),
        };
        let mut mocker = Mocker::new(block);
        mocker.input().set(input.clone());
        mocker.output().reserve(n_samp);
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        b.iter(move || {
            async_io::block_on(async {
                for _ in 0..N {
                    let _ = black_box(
                        mocker
                            .block
                            .kernel
                            .work(&mut io, &mut mocker.block.mio, &mut mocker.block.meta)
                            .await,
                    );
                }
            })
        });
    });

    group.finish();
}

criterion_group!(benches, sync_vs_async);
criterion_main!(benches);
