use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futuredsp::fir::NonResamplingFirKernel;
use futuredsp::iir::IirKernel;
use futuredsp::{StatefulUnaryKernel, TapsAccessor, UnaryKernel};
use num_complex::Complex;
use rand::Rng;

extern crate alloc;
use alloc::vec::Vec;

trait Generatable {
    fn generate() -> Self;
}

impl Generatable for f32 {
    fn generate() -> Self {
        let mut rng = rand::thread_rng();
        rng.gen::<f32>() * 2.0 - 1.0
    }
}

impl Generatable for Complex<f32> {
    fn generate() -> Self {
        let mut rng = rand::thread_rng();
        Complex {
            re: rng.gen::<f32>() * 2.0 - 1.0,
            im: rng.gen::<f32>() * 2.0 - 1.0,
        }
    }
}

fn bench_fir_dynamic_taps<InputType, OutputType, TapType: Generatable>(
    b: &mut criterion::Bencher,
    ntaps: usize,
    nsamps: usize,
) where
    InputType: Generatable + Clone,
    OutputType: Generatable + Clone,
    Vec<TapType>: TapsAccessor<TapType = TapType>,
    NonResamplingFirKernel<InputType, OutputType, Vec<TapType>, TapType>:
        UnaryKernel<InputType, OutputType>,
{
    let taps: Vec<_> = (0..ntaps).map(|_| TapType::generate()).collect();
    let input: Vec<_> = (0..nsamps + ntaps).map(|_| InputType::generate()).collect();
    let mut output = vec![OutputType::generate(); nsamps];
    let fir = NonResamplingFirKernel::<InputType, OutputType, _, _>::new(black_box(taps));
    b.iter(|| {
        fir.work(black_box(&input), black_box(&mut output));
    });
}

fn bench_fir_static_taps<InputType, OutputType, TapType: Generatable, const N: usize>(
    b: &mut criterion::Bencher,
    nsamps: usize,
) where
    InputType: Generatable + Clone,
    OutputType: Generatable + Clone,
    TapType: std::fmt::Debug,
    [TapType; N]: TapsAccessor<TapType = TapType>,
    NonResamplingFirKernel<InputType, OutputType, [TapType; N], TapType>:
        UnaryKernel<InputType, OutputType>,
{
    let taps: Vec<_> = (0..N).map(|_| TapType::generate()).collect();
    let taps: [TapType; N] = taps.try_into().unwrap();
    let input: Vec<_> = (0..nsamps + N).map(|_| InputType::generate()).collect();
    let mut output = vec![OutputType::generate(); nsamps];
    let fir = NonResamplingFirKernel::<InputType, OutputType, _, _>::new(black_box(taps));
    b.iter(|| {
        fir.work(black_box(&input), black_box(&mut output));
    });
}

fn bench_iir<InputType, OutputType, TapType: Generatable>(
    b: &mut criterion::Bencher,
    n_a_taps: usize,
    n_b_taps: usize,
    nsamps: usize,
) where
    InputType: Generatable + Clone,
    OutputType: Generatable + Clone,
    Vec<TapType>: TapsAccessor<TapType = TapType>,
    IirKernel<InputType, OutputType, Vec<TapType>>: StatefulUnaryKernel<InputType, OutputType>,
{
    let a_taps: Vec<_> = (0..n_a_taps).map(|_| TapType::generate()).collect();
    let b_taps: Vec<_> = (0..n_b_taps).map(|_| TapType::generate()).collect();
    let input: Vec<_> = (0..nsamps + n_b_taps)
        .map(|_| InputType::generate())
        .collect();
    let mut output = vec![OutputType::generate(); nsamps];
    let mut iir = IirKernel::new(black_box(a_taps), black_box(b_taps));
    b.iter(|| {
        iir.work(black_box(&input), black_box(&mut output));
    });
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let nsamps = 1000usize;

    let mut group = c.benchmark_group("fir");

    group.throughput(criterion::Throughput::Elements(nsamps as u64));

    for ntaps in [3, 64] {
        group.bench_function(
            format!("fir-{}tap-dynamic real/real {}", ntaps, nsamps),
            |b| {
                bench_fir_dynamic_taps::<f32, f32, f32>(b, ntaps, nsamps);
            },
        );
        group.bench_function(
            format!("fir-{}tap-dynamic complex/real {}", ntaps, nsamps),
            |b| {
                bench_fir_dynamic_taps::<Complex<f32>, Complex<f32>, f32>(b, ntaps, nsamps);
            },
        );
    }

    // Check some static taps as well
    group.bench_function(format!("fir-3tap-static complex/real {}", nsamps), |b| {
        bench_fir_static_taps::<Complex<f32>, Complex<f32>, f32, 3>(b, nsamps);
    });
    group.bench_function(format!("fir-64tap-static complex/real {}", nsamps), |b| {
        bench_fir_static_taps::<Complex<f32>, Complex<f32>, f32, 64>(b, nsamps);
    });

    group.finish();

    let mut group = c.benchmark_group("iir");
    group.throughput(criterion::Throughput::Elements(nsamps as u64));

    group.bench_function("iir", |b| {
        bench_iir(b, 7, 1, nsamps);
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
