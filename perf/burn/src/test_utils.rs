use futuresdr::runtime::dev::prelude::Complex32;
use rustfft::FftPlanner;

use crate::FFT_SIZE;

pub fn nontrivial_input(batch_size: usize) -> Vec<Complex32> {
    let mut state = 0x6d2b_79f5_u32;
    (0..batch_size * FFT_SIZE)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let re = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let im = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            Complex32::new(re, im)
        })
        .collect()
}

pub fn reference_spectrum(input: &[Complex32], batch_size: usize) -> Vec<f32> {
    assert_eq!(input.len(), batch_size * FFT_SIZE);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut accumulated = vec![0.0_f32; FFT_SIZE];

    for frame in input.as_chunks::<FFT_SIZE>().0 {
        let mut values = frame.to_vec();
        fft.process(&mut values);
        for (sum, value) in accumulated.iter_mut().zip(values) {
            *sum += value.norm_sqr();
        }
    }

    let mut output = vec![0.0_f32; FFT_SIZE];
    for (index, sum) in accumulated.into_iter().enumerate() {
        let shifted = (index + FFT_SIZE / 2) % FFT_SIZE;
        output[shifted] = (sum / batch_size as f32 + 1.0e-30).log10();
    }
    output
}

pub fn assert_spectrum_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        let tolerance = 0.02 + 0.02 * expected.abs();
        assert!(
            (actual - expected).abs() <= tolerance,
            "spectrum bin {index} differs: actual={actual}, expected={expected}, tolerance={tolerance}"
        );
    }
}
