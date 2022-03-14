//! Methods for designing FIR filters.

extern crate alloc;
use crate::math::consts;
use crate::windows::FilterWindow;
use alloc::vec::Vec;

/// Constructs a lowpass FIR filter with unit gain and cutoff frequency `cutoff` (in cycles/sample)
/// using the specified window.
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let cutoff = 2_000.0 / sampling_freq as f32;
/// let num_taps = 65;
/// let rect_win = windows::RectWindow::new(num_taps);
/// let taps = firdes::lowpass(num_taps, cutoff, rect_win);
/// ```
pub fn lowpass<T: FilterWindow<f32>>(num_taps: usize, cutoff: f32, window: T) -> Vec<f32> {
    assert!(num_taps > 0, "num_taps must be greater than 0");
    assert!(
        cutoff > 0.0 && cutoff < 1.0 / 2.0,
        "cutoff must be in (0, 1/2)"
    );
    let mut taps: Vec<f32> = Vec::with_capacity(num_taps);
    let omega_c = 2.0 * consts::f32::PI * cutoff;
    let alpha = (num_taps - 1) as f32 / 2.0;
    for n in 0..num_taps {
        let x = n as f32 - alpha;
        let tap = match x == 0.0 {
            true => 1.0,
            false => (omega_c * x).sin() / (consts::f32::PI * x),
        };
        taps.push(tap * window.get(n));
    }

    taps
}

/// Constructs a highpass FIR filter with unit gain and cutoff frequency `cutoff` (in cycles/sample)
/// using the specified window.
/// Note that `num_taps` must be odd, otherwise one tap is added to the generated filter.
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let cutoff = 4_000.0 / sampling_freq as f32;
/// let num_taps = 65;
/// let rect_win = windows::RectWindow::new(num_taps);
/// let taps = firdes::highpass(num_taps, cutoff, rect_win);
/// ```
pub fn highpass<T: FilterWindow<f32>>(num_taps: usize, cutoff: f32, window: T) -> Vec<f32> {
    assert!(num_taps > 0, "num_taps must be greater than 0");
    assert!(
        cutoff > 0.0 && cutoff < 1.0 / 2.0,
        "cutoff must be in (0, 1/2)"
    );
    // Number of taps must be odd
    let num_taps = match num_taps % 2 {
        0 => {
            // println!("Warning: num_taps must be odd. Adding one.");
            num_taps + 1
        }
        _ => num_taps,
    };
    let mut taps: Vec<f32> = Vec::with_capacity(num_taps);
    let omega_c = 2.0 * consts::f32::PI * cutoff;
    let alpha = (num_taps - 1) as f32 / 2.0;
    for n in 0..num_taps {
        let x = n as f32 - alpha;
        let tap = match x == 0.0 {
            true => 1.0 - omega_c / consts::f32::PI,
            false => -(omega_c * x).sin() / (consts::f32::PI * x),
        };
        taps.push(tap * window.get(n));
    }
    taps
}

/// Constructs a bandpass FIR filter with unit gain and cutoff frequencies
/// `lower_cutoff` and `higher_cutoff` (in cycles/sample) using the specified window.
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let lower_cutoff = 2_000.0 / sampling_freq as f32;
/// let higher_cutoff = 4_000.0 / sampling_freq as f32;
/// let num_taps = 65;
/// let rect_win = windows::RectWindow::new(num_taps);
/// let taps = firdes::bandpass(num_taps, lower_cutoff, higher_cutoff, rect_win);
/// ```
pub fn bandpass<T: FilterWindow<f32>>(
    num_taps: usize,
    lower_cutoff: f32,
    higher_cutoff: f32,
    window: T,
) -> Vec<f32> {
    assert!(num_taps > 0, "num_taps must be greater than 0");
    assert!(
        lower_cutoff > 0.0 && lower_cutoff < 1.0 / 2.0,
        "lower_cutoff must be in (0, 1/2)"
    );
    assert!(
        higher_cutoff > lower_cutoff && higher_cutoff < 1.0 / 2.0,
        "higher_cutoff must be in (lower_cutoff, 1/2)"
    );
    let mut taps: Vec<f32> = Vec::with_capacity(num_taps);
    let lower_omega_c = 2.0 * consts::f32::PI * lower_cutoff;
    let higher_omega_c = 2.0 * consts::f32::PI * higher_cutoff;
    let omega_passband_bw = higher_omega_c - lower_omega_c;
    let omega_passband_center = (lower_omega_c + higher_omega_c) / 2.0;
    let alpha = (num_taps - 1) as f32 / 2.0;
    for n in 0..num_taps {
        let x = n as f32 - alpha;
        let tap = match x == 0.0 {
            true => omega_passband_bw / consts::f32::PI,
            false => {
                2.0 * (omega_passband_center * x).cos() * (omega_passband_bw / 2.0 * x).sin()
                    / (consts::f32::PI * x)
            }
        };
        taps.push(tap * window.get(n));
    }
    taps
}
