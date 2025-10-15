use criterion::BatchSize;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use futuresdr::runtime::mocker;
use std::iter::repeat_with;

use futuresdr::blocks::Apply;
use futuresdr::prelude::*;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

#[derive(Block)]
struct Add {
    #[input]
    input: mocker::Reader<u8>,
    #[output]
    output: mocker::Writer<u8>,
}

impl Kernel for Add {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let n = std::cmp::min(i_len, o.len());

        if n > 0 {
            for (i, o) in i.iter().zip(o.iter_mut()) {
                *o = i.wrapping_add(1);
            }

            self.input.consume(n);
            self.output.produce(n);
        }

        if self.input.finished() && i_len == n {
            io.finished = true;
        }

        Ok(())
    }
}

#[derive(Block)]
struct AddChunk {
    #[input]
    input: mocker::Reader<u8>,
    #[output]
    output: mocker::Writer<u8>,
}

impl Kernel for AddChunk {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let n = std::cmp::min(i_len, o.len());

        if n > 0 {
            for (i, o) in i.chunks_exact(32).zip(o.chunks_exact_mut(32)) {
                for x in 0..32 {
                    o[x] = i[x].wrapping_add(1);
                }
            }

            self.input.consume(n);
            self.output.produce(n);
        }

        if self.input.finished() && i_len == n {
            io.finished = true;
        }

        Ok(())
    }
}

#[cfg(target_feature = "avx2")]
mod avx2 {
    use super::*;
    use std::arch::x86_64::__m256i;
    use std::arch::x86_64::_mm256_add_epi8;
    use std::arch::x86_64::_mm256_loadu_si256;
    use std::arch::x86_64::_mm256_set1_epi8;
    use std::arch::x86_64::_mm256_storeu_si256;

    #[derive(Block)]
    pub struct AddAvx2 {
        #[input]
        pub input: mocker::Reader<u8>,
        #[output]
        pub output: mocker::Writer<u8>,
    }

    impl Kernel for AddAvx2 {
        async fn work(
            &mut self,
            io: &mut WorkIo,
            _m: &mut MessageOutputs,
            _b: &mut BlockMeta,
        ) -> Result<()> {
            let i = self.input.slice();
            let o = self.output.slice();
            let i_len = i.len();

            let n = std::cmp::min(i_len, o.len());

            if n > 0 {
                unsafe {
                    let ones: __m256i = _mm256_set1_epi8(1);

                    for (i, o) in i.chunks_exact(32).zip(o.chunks_exact_mut(32)) {
                        // load 32 bytes (unaligned)
                        let v = _mm256_loadu_si256(i.as_ptr() as *const __m256i);
                        // add 1 to each lane, wrapping modulo 256
                        let r = _mm256_add_epi8(v, ones);
                        // store back
                        _mm256_storeu_si256(o.as_mut_ptr() as *mut __m256i, r);
                    }
                }

                self.input.consume(n);
                self.output.produce(n);
            }

            if self.input.finished() && i_len == n {
                io.finished = true;
            }

            Ok(())
        }
    }
}

pub fn apply(c: &mut Criterion) {
    let n_samp = 1024 * 1024;
    let input: Vec<u8> = repeat_with(rand::random::<u8>).take(n_samp).collect();

    let mut group = c.benchmark_group("apply");

    group.throughput(criterion::Throughput::Elements(n_samp as u64));

    group.bench_function(format!("apply-u8-plus-1-{n_samp}"), |b| {
        b.iter_batched(
            || {
                let block: Apply<_, _, _, Reader<u8>, Writer<u8>> =
                    Apply::new(|x: &u8| x.wrapping_add(1));
                let mut mocker = Mocker::new(block);
                mocker.input().set(input.clone());
                mocker.output().reserve(n_samp);
                mocker
            },
            |mut m: _| m.run(),
            BatchSize::LargeInput,
        );
    });

    group.bench_function(format!("block-u8-plus-1-{n_samp}"), |b| {
        b.iter_batched(
            || {
                let block = Add {
                    input: Default::default(),
                    output: Default::default(),
                };
                let mut mocker = Mocker::new(block);
                mocker.input().set(input.clone());
                mocker.output().reserve(n_samp);
                mocker
            },
            |mut m: _| m.run(),
            BatchSize::LargeInput,
        );
    });

    group.bench_function(format!("chunks-u8-plus-1-{n_samp}"), |b| {
        b.iter_batched(
            || {
                let block = AddChunk {
                    input: Default::default(),
                    output: Default::default(),
                };
                let mut mocker = Mocker::new(block);
                mocker.input().set(input.clone());
                mocker.output().reserve(n_samp);
                mocker
            },
            |mut m: _| m.run(),
            BatchSize::LargeInput,
        );
    });

    #[cfg(target_feature = "avx2")]
    group.bench_function(format!("avx2-u8-plus-1-{n_samp}"), |b| {
        b.iter_batched(
            || {
                let block = avx2::AddAvx2 {
                    input: Default::default(),
                    output: Default::default(),
                };
                let mut mocker = Mocker::new(block);
                mocker.input().set(input.clone());
                mocker.output().reserve(n_samp);
                mocker
            },
            |mut m: _| m.run(),
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(benches, apply);
criterion_main!(benches);
