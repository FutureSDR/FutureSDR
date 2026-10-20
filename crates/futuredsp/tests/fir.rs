use futuredsp::ComputationStatus;
use futuredsp::Filter;
use futuredsp::FirFilter;
use num_complex::Complex;

fn check_boundaries<S, T>(
    sample: impl Fn(usize) -> S,
    tap: impl Fn(usize) -> T,
    widen_sample: impl Fn(S) -> Complex<f64>,
    widen_tap: impl Fn(T) -> Complex<f64>,
    tolerance: f64,
) where
    S: Copy,
    T: Copy + Send + Sync,
    FirFilter<S, S, Vec<T>>: Filter<S, S, T>,
{
    for tap_count in [1, 2, 3, 31, 32, 33, 127] {
        let taps: Vec<_> = (0..tap_count).map(&tap).collect();
        let fir = FirFilter::<S, S, _>::new(taps.clone());
        for input_count in [
            0,
            tap_count - 1,
            tap_count,
            tap_count + 14,
            tap_count + 15,
            tap_count + 16,
            tap_count + 30,
            tap_count + 31,
            tap_count + 32,
            tap_count + 63,
            tap_count + 128,
        ] {
            let input: Vec<_> = (0..input_count).map(&sample).collect();
            let available = input_count.saturating_sub(tap_count - 1);
            for capacity in [0, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 129] {
                let sentinel = sample(997);
                let mut output = vec![sentinel; capacity];
                let n = available.min(capacity);
                let status = match available.cmp(&capacity) {
                    std::cmp::Ordering::Less => ComputationStatus::InsufficientInput,
                    std::cmp::Ordering::Equal => ComputationStatus::BothSufficient,
                    std::cmp::Ordering::Greater => ComputationStatus::InsufficientOutput,
                };
                assert_eq!(fir.filter(&input, &mut output), (n, n, status));
                for (k, &actual) in output[..n].iter().enumerate() {
                    let expected: Complex<f64> = input[k..k + tap_count]
                        .iter()
                        .zip(taps.iter().rev())
                        .map(|(&s, &t)| widen_sample(s) * widen_tap(t))
                        .sum();
                    assert!(
                        (widen_sample(actual) - expected).norm() < tolerance,
                        "{tap_count} taps, {input_count} inputs, capacity {capacity}, output {k}"
                    );
                }
                for &untouched in &output[n..] {
                    assert_eq!(widen_sample(untouched), widen_sample(sentinel));
                }
            }
        }
    }
}

fn real(k: usize) -> f64 {
    (k * 37 % 101) as f64 / 101.0 - 0.5
}

fn complex(k: usize) -> Complex<f64> {
    Complex::new(real(k), real(k + 13))
}

fn complex_f32(k: usize) -> Complex<f32> {
    Complex::new(real(k) as f32, real(k + 13) as f32)
}

#[test]
fn real_f32_boundaries() {
    check_boundaries(
        |k| real(k) as f32,
        |k| real(k + 7) as f32,
        |s| Complex::new(f64::from(s), 0.0),
        |t| Complex::new(f64::from(t), 0.0),
        1e-5,
    );
}

#[test]
fn real_f64_boundaries() {
    check_boundaries(
        real,
        |k| real(k + 7),
        |s| Complex::new(s, 0.0),
        |t| Complex::new(t, 0.0),
        1e-12,
    );
}

#[test]
fn complex_f32_real_taps_boundaries() {
    check_boundaries(
        complex_f32,
        |k| real(k + 7) as f32,
        |s| Complex::new(f64::from(s.re), f64::from(s.im)),
        |t| Complex::new(f64::from(t), 0.0),
        1e-5,
    );
}

#[test]
fn complex_f64_real_taps_boundaries() {
    check_boundaries(
        complex,
        |k| real(k + 7),
        |s| s,
        |t| Complex::new(t, 0.0),
        1e-12,
    );
}

#[test]
fn complex_f32_complex_taps_boundaries() {
    check_boundaries(
        complex_f32,
        |k| complex_f32(k + 7),
        |s| Complex::new(f64::from(s.re), f64::from(s.im)),
        |t| Complex::new(f64::from(t.re), f64::from(t.im)),
        1e-5,
    );
}

#[test]
fn static_and_borrowed_taps_match_reference() {
    let taps: [f32; 33] = std::array::from_fn(|k| real(k + 7) as f32);
    let input: Vec<_> = (0..97).map(complex_f32).collect();
    let owned = FirFilter::<Complex<f32>, Complex<f32>, _>::new(taps);
    let borrowed = FirFilter::<Complex<f32>, Complex<f32>, _>::new(&taps);
    let mut output = [Complex::default(); 65];
    let mut borrowed_output = output;
    let expected_status = (65, 65, ComputationStatus::BothSufficient);
    assert_eq!(owned.filter(&input, &mut output), expected_status);
    assert_eq!(
        borrowed.filter(&input, &mut borrowed_output),
        expected_status
    );
    for (k, (&owned, &borrowed)) in output.iter().zip(&borrowed_output).enumerate() {
        let expected: Complex<f32> = input[k..k + taps.len()]
            .iter()
            .zip(taps.iter().rev())
            .map(|(s, t)| s * t)
            .sum();
        assert!((owned - expected).norm() < 1e-5);
        assert!((borrowed - expected).norm() < 1e-5);
    }
}

#[test]
fn empty_taps_preserve_counts_and_zero_outputs() {
    let fir = FirFilter::<f32, f32, [f32; 0]>::new([]);
    for input_count in [0, 1, 31, 32, 33] {
        let input = vec![1.0; input_count];
        for capacity in [0, 1, 31, 32, 33, 65] {
            let mut output = vec![7.0; capacity];
            let n = (input_count + 1).min(capacity);
            let status = match (input_count + 1).cmp(&capacity) {
                std::cmp::Ordering::Less => ComputationStatus::InsufficientInput,
                std::cmp::Ordering::Equal => ComputationStatus::BothSufficient,
                std::cmp::Ordering::Greater => ComputationStatus::InsufficientOutput,
            };
            assert_eq!(fir.filter(&input, &mut output), (n, n, status));
            assert!(output[..n].iter().all(|&x| x == 0.0));
            assert!(output[n..].iter().all(|&x| x == 7.0));
        }
    }
}

#[test]
fn streaming_with_small_buffers_matches_one_shot() {
    let taps: Vec<_> = (0..127).map(|k| real(k + 7) as f32).collect();
    let fir = FirFilter::<Complex<f32>, Complex<f32>, _>::new(taps);
    let input: Vec<_> = (0..2053).map(complex_f32).collect();
    let mut expected = vec![Complex::default(); input.len() - fir.length() + 1];
    let (consumed, produced, status) = fir.filter(&input, &mut expected);
    assert_eq!((consumed, produced), (expected.len(), expected.len()));
    assert_eq!(status, ComputationStatus::BothSufficient);

    let mut pending = Vec::new();
    let mut actual = Vec::new();
    let mut position = 0;
    let mut capacities = [1, 31, 32, 33, 63, 64, 65].into_iter().cycle();
    for chunk in [1, 7, 31, 32, 33, 257].into_iter().cycle() {
        let end = (position + chunk).min(input.len());
        pending.extend_from_slice(&input[position..end]);
        position = end;
        loop {
            let mut output = vec![Complex::default(); capacities.next().unwrap()];
            let (consumed, produced, status) = fir.filter(&pending, &mut output);
            assert_eq!(consumed, produced);
            actual.extend_from_slice(&output[..produced]);
            pending.drain(..consumed);
            if status != ComputationStatus::InsufficientOutput {
                break;
            }
            assert_ne!(consumed, 0);
        }
        if position == input.len() {
            break;
        }
    }
    assert_eq!(pending.len(), fir.length() - 1);
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((*actual - expected).norm() < 1e-5);
    }
}
