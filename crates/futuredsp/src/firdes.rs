//! Methods for designing FIR filters.

extern crate alloc;
use alloc::vec::Vec;
use num_traits::FromPrimitive;

/// Constructs a lowpass FIR filter with unit gain and cutoff frequency `cutoff` (in cycles/sample)
/// using the specified window. The length of the filter equals the length of `window`.
/// The filter taps are constructed internally as `f64` and then casted to the generic type `T`
/// using [`num_traits::FromPrimitive::from_f64()`].
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let cutoff = 2_000.0 / sampling_freq as f64;
/// let num_taps = 65;
/// let rect_win = windows::rect(num_taps);
/// let taps = firdes::lowpass::<f32>(cutoff, rect_win.as_slice());
/// ```
pub fn lowpass<T: FromPrimitive>(cutoff: f64, window: &[f64]) -> Vec<T> {
    assert!(
        cutoff > 0.0 && cutoff < 1.0 / 2.0,
        "cutoff must be in (0, 1/2)"
    );
    let omega_c = 2.0 * core::f64::consts::PI * cutoff;
    let alpha = (window.len() - 1) as f64 / 2.0;
    window
        .iter()
        .enumerate()
        .map(|(n, tap)| {
            let x = n as f64 - alpha;
            let filter_tap = match x == 0.0 {
                true => omega_c / core::f64::consts::PI,
                false => (omega_c * x).sin() / (core::f64::consts::PI * x),
            };
            tap * filter_tap
        })
        .map(|x| T::from_f64(x).unwrap())
        .collect()
}

/// Constructs a highpass FIR filter with unit gain and cutoff frequency `cutoff` (in cycles/sample)
/// using the specified window.  The length of the filter equals the length of `window`.
/// Note that `window.len()` must be odd.
/// The filter taps are constructed internally as `f64` and then casted to the generic type `T`
/// using [`num_traits::FromPrimitive::from_f64()`].
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let cutoff = 4_000.0 / sampling_freq as f64;
/// let num_taps = 65;
/// let rect_win = windows::rect(num_taps);
/// let taps = firdes::highpass::<f32>(cutoff, rect_win.as_slice());
/// ```
pub fn highpass<T: FromPrimitive>(cutoff: f64, window: &[f64]) -> Vec<T> {
    assert!(
        cutoff > 0.0 && cutoff < 1.0 / 2.0,
        "cutoff must be in (0, 1/2)"
    );
    assert!(window.len() % 2 == 1, "window.len() must be odd");
    let omega_c = 2.0 * core::f64::consts::PI * cutoff;
    let alpha = (window.len() - 1) as f64 / 2.0;
    window
        .iter()
        .enumerate()
        .map(|(n, tap)| {
            let x = n as f64 - alpha;
            let filter_tap = match x == 0.0 {
                true => 1.0 - omega_c / core::f64::consts::PI,
                false => -(omega_c * x).sin() / (core::f64::consts::PI * x),
            };
            tap * filter_tap
        })
        .map(|x| T::from_f64(x).unwrap())
        .collect()
}

/// Constructs a bandpass FIR filter with unit gain and cutoff frequencies
/// `lower_cutoff` and `higher_cutoff` (in cycles/sample) using the specified window.
///  The length of the filter equals the length of `window`.
/// The filter taps are constructed internally as `f64` and then casted to the generic type `T`
/// using [`num_traits::FromPrimitive::from_f64()`].
///
/// Example usage:
/// ```
/// use futuredsp::{firdes, windows};
///
/// let sampling_freq = 10_000;
/// // 2000 Hz cutoff frequency, rectangular window
/// let lower_cutoff = 2_000.0 / sampling_freq as f64;
/// let higher_cutoff = 4_000.0 / sampling_freq as f64;
/// let num_taps = 65;
/// let rect_win = windows::rect(num_taps);
/// let taps = firdes::bandpass::<f32>(lower_cutoff, higher_cutoff, rect_win.as_slice());
/// ```
pub fn bandpass<T: FromPrimitive>(lower_cutoff: f64, higher_cutoff: f64, window: &[f64]) -> Vec<T> {
    assert!(
        lower_cutoff > 0.0 && lower_cutoff < 1.0 / 2.0,
        "lower_cutoff must be in (0, 1/2)"
    );
    assert!(
        higher_cutoff > lower_cutoff && higher_cutoff < 1.0 / 2.0,
        "higher_cutoff must be in (lower_cutoff, 1/2)"
    );
    let lower_omega_c = 2.0 * core::f64::consts::PI * lower_cutoff;
    let higher_omega_c = 2.0 * core::f64::consts::PI * higher_cutoff;
    let omega_passband_bw = higher_omega_c - lower_omega_c;
    let omega_passband_center = (lower_omega_c + higher_omega_c) / 2.0;
    let alpha = (window.len() - 1) as f64 / 2.0;
    window
        .iter()
        .enumerate()
        .map(|(n, tap)| {
            let x = n as f64 - alpha;
            let filter_tap = match x == 0.0 {
                true => omega_passband_bw / core::f64::consts::PI,
                false => {
                    2.0 * (omega_passband_center * x).cos() * (omega_passband_bw / 2.0 * x).sin()
                        / (core::f64::consts::PI * x)
                }
            };
            tap * filter_tap
        })
        .map(|x| T::from_f64(x).unwrap())
        .collect()
}

/// Constructs a root raised cosine filter with roll-off factor `roll_off`, truncated to
/// `span` symbols. Each symbol is represented using `sps` samples. `span * sps` must be
/// even. The returned filter has a length `span * sps + 1`.
/// The filter taps are constructed internally as `f64` and then casted to the generic type `T`
/// using [`num_traits::FromPrimitive::from_f64()`].
///
/// Example usage:
/// ```
/// use futuredsp::firdes;
///
/// let span = 8;
/// let sps = 4;
/// let roll_off = 0.25;
/// let taps = firdes::root_raised_cosine::<f32>(span, sps, roll_off);
/// ```
pub fn root_raised_cosine<T: FromPrimitive>(span: usize, sps: usize, roll_off: f64) -> Vec<T> {
    assert!((span * sps) % 2 == 0, "span * sps must be even");
    assert!(
        roll_off > 0.0 && roll_off <= 1.0,
        "roll_off must be in (0,1]"
    );
    let num_taps = span * sps + 1;
    let mut taps = Vec::<f64>::with_capacity(num_taps);
    for n in 0..num_taps {
        let t = (n as f64 - (num_taps - 1) as f64 / 2.0) / sps as f64;
        let tap = match t {
            t if t == 0.0 => {
                ((1.0 - roll_off) + (4.0 * roll_off / core::f64::consts::PI)) / (sps as f64).sqrt()
            }
            t if (t.abs() - (4.0 * roll_off).recip()).abs() < 1e-5 => {
                roll_off / ((2.0f64 * sps as f64).sqrt())
                    * ((1.0 + 2.0 / core::f64::consts::PI)
                        * (core::f64::consts::PI / (4.0 * roll_off)).sin()
                        + (1.0 - 2.0 / core::f64::consts::PI)
                            * (core::f64::consts::PI / (4.0 * roll_off)).cos())
            }
            _ => {
                let tmp = 4.0 * roll_off * t;
                (((1.0 - roll_off) * core::f64::consts::PI * t).sin()
                    + tmp * ((1.0 + roll_off) * core::f64::consts::PI * t).cos())
                    / (core::f64::consts::PI * t * (1.0 - tmp.powi(2)) * (sps as f64).sqrt())
            }
        };
        taps.push(tap);
    }
    taps.iter().map(|x| T::from_f64(*x).unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_raised_cosine_accuracy() {
        let span = 6;
        let sps = 8;
        let roll_off = 0.2;
        // Test taps generated using matlab:
        // ```
        // taps = rcosdesign(0.2, 8, 6, 'sqrt')
        // ```
        let test_taps = [
            -0.0134, -0.0041, 0.0075, 0.0197, 0.0301, 0.0364, 0.0368, 0.0302, 0.0165, -0.0029,
            -0.0255, -0.0478, -0.0654, -0.0744, -0.0709, -0.0525, -0.0186, 0.0297, 0.0894, 0.1555,
            0.2222, 0.2829, 0.3314, 0.3628, 0.3736, 0.3628, 0.3314, 0.2829, 0.2222, 0.1555, 0.0894,
            0.0297, -0.0186, -0.0525, -0.0709, -0.0744, -0.0654, -0.0478, -0.0255, -0.0029, 0.0165,
            0.0302, 0.0368, 0.0364, 0.0301, 0.0197, 0.0075, -0.0041, -0.0134,
        ];
        let filter_taps = root_raised_cosine::<f64>(span, sps, roll_off);
        assert_eq!(filter_taps.len(), test_taps.len());
        for i in 0..filter_taps.len() {
            let tol = 1e-2;
            assert!(
                (filter_taps[i] - test_taps[i]).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                filter_taps[i],
                test_taps[i],
                tol,
                i
            );
        }
    }
}

/// FIR filter design methods based on the Kaiser window method. The resulting
/// filters have generalized linear phase.
///
/// The Kaiser method is described in:
/// - J. F. Kaiser "Nonrecursive Digital Filter Design using the I_0-sinh
///   Window Function," Proc. 1974 IEEE International Symposium on Circuits
///   & Systems, San Francisco CA, April. 1974.
/// - A. V. Oppenheim and R. W. Schafer "Digital Signal Processing," 3rd Edition.
pub mod kaiser {
    extern crate alloc;
    use crate::windows::kaiser;
    use alloc::vec::Vec;
    use num_traits::FromPrimitive;

    /// Designs a lowpass FIR filter with cutoff frequency `cutoff` and
    /// transition width `transition_bw` (in cycles/sample).
    /// The number of taps in the filter depends on the specifications.
    ///
    /// Example usage:
    /// ```
    /// use futuredsp::firdes;
    ///
    /// let sampling_freq = 10_000;
    /// // 2000 Hz cutoff frequency and 500 Hz transtion band
    /// let cutoff = 2_000.0 / sampling_freq as f64;
    /// let transition_bw = 500.0 / sampling_freq as f64;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, max_ripple);
    /// ```
    pub fn lowpass<T: FromPrimitive>(cutoff: f64, transition_bw: f64, max_ripple: f64) -> Vec<T> {
        assert!(cutoff > 0.0, "cutoff must be greater than 0");
        assert!(transition_bw > 0.0, "transition_bw must be greater than 0");
        assert!(
            cutoff + transition_bw < 1.0 / 2.0,
            "cutoff+transition_bw must be less than 1/2"
        );
        let (num_taps, beta) = design_kaiser_window(transition_bw, max_ripple);
        let win = kaiser(num_taps, beta);
        let omega_c = (2.0 * cutoff + transition_bw) / 2.0;
        super::lowpass(omega_c, win.as_slice())
    }

    /// Designs a highpass FIR filter with cutoff frequency `cutoff` and
    /// transition width `transition_bw` (in cycles/sample).
    /// The number of taps in the filter depends on the specifications.
    ///
    /// Example usage:
    /// ```
    /// use futuredsp::firdes;
    ///
    /// let sampling_freq = 10_000;
    /// // 4000 Hz cutoff frequency and 500 Hz transtion band
    /// let cutoff = 4_000.0 / sampling_freq as f64;
    /// let transition_bw = 500.0 / sampling_freq as f64;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::highpass::<f32>(cutoff, transition_bw, max_ripple);
    /// ```
    pub fn highpass<T: FromPrimitive>(cutoff: f64, transition_bw: f64, max_ripple: f64) -> Vec<T> {
        assert!(cutoff > 0.0, "cutoff must be greater than 0");
        assert!(transition_bw > 0.0, "transition_bw must be greater than 0");
        assert!(
            cutoff + transition_bw < 1.0 / 2.0,
            "cutoff+transition_bw must be less than 1/2"
        );
        // Determine cutoff frequency of the underlying ideal lowpass filter
        let (num_taps, beta) = design_kaiser_window(transition_bw, max_ripple);
        // Number of taps must be odd
        let num_taps = num_taps + ((num_taps + 1) % 2);
        let win = kaiser(num_taps, beta);
        let omega_c = (2.0 * cutoff - transition_bw) / 2.0;
        super::highpass(omega_c, win.as_slice())
    }

    /// Designs a bandpass FIR filter with lower cutoff frequency `lower_cutoff`,
    /// higher cutoff frequency `higher_cutoff`, and transition widths
    /// `transition_bw` (in cycles/sample).
    /// The number of taps in the filter depends on the specifications.
    ///
    /// Example usage:
    /// ```
    /// use futuredsp::firdes;
    ///
    /// let sampling_freq = 10_000;
    /// // 1000 Hz lower cutoff frequency, 2000 Hz higher cutoff frequency,
    /// // and 500 Hz transtion bands
    /// let lower_cutoff = 1_000.0 / sampling_freq as f64;
    /// let higher_cutoff = 4_000.0 / sampling_freq as f64;
    /// let transition_bw = 500.0 / sampling_freq as f64;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::bandpass::<f32>(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
    /// ```
    pub fn bandpass<T: FromPrimitive>(
        lower_cutoff: f64,
        higher_cutoff: f64,
        transition_bw: f64,
        max_ripple: f64,
    ) -> Vec<T> {
        assert!(lower_cutoff > 0.0, "lower_cutoff must be greater than 0");
        assert!(
            higher_cutoff > lower_cutoff,
            "higher_cutoff must be greater than lower_cutoff"
        );
        assert!(transition_bw > 0.0, "transition_bw must be greater than 0");
        assert!(
            higher_cutoff + transition_bw < 1.0 / 2.0,
            "higher_cutoff+transition_bw must be less than 1/2"
        );
        let (num_taps, beta) = design_kaiser_window(transition_bw, max_ripple);
        let win = kaiser(num_taps, beta);
        let lower_omega_c = (2.0 * lower_cutoff - transition_bw) / 2.0;
        let higher_omega_c = (2.0 * higher_cutoff + transition_bw) / 2.0;
        super::bandpass(lower_omega_c, higher_omega_c, win.as_slice())
    }

    /// Designs a Nyquist FIR filter (L-th band filter) for polyphase resampling with
    /// interpolation factor `interp` and decimation factor `decim`. Each polyphase
    /// filter will contain `2 * half_polyphase_len` taps.
    ///
    /// Setting `half_polyphase_len = 12` and `max_ripple = 0.0001` seems
    /// reasonable for most applications.
    ///
    /// Example usage:
    /// ```
    /// use futuredsp::firdes;
    ///
    /// let taps = firdes::kaiser::multirate::<f32>(3, 2, 12, 0.0001);
    /// ```
    pub fn multirate<T: FromPrimitive>(
        interp: usize,
        decim: usize,
        half_polyphase_len: usize,
        max_ripple: f64,
    ) -> Vec<T> {
        assert!(interp > 0, "interp must be greater than 0");
        assert!(decim > 0, "decim must be greater than 0");
        assert!(
            half_polyphase_len > 0,
            "polyphase_taps must be greater than 0"
        );
        if interp == 1 && decim == 1 {
            return vec![T::from_f64(1.0).unwrap()];
        }
        let band = match interp {
            1 => decim,
            _ => interp,
        };
        let num_taps = 2 * half_polyphase_len * band;
        let beta = compute_kaiser_beta(max_ripple);
        // Scale window by interp to get unit gain
        let win: Vec<f64> = kaiser(num_taps + 1, beta)
            .iter()
            .map(|x| interp as f64 * x)
            .collect();
        let omega_c = 1.0 / (2.0 * core::cmp::max(interp, decim) as f64);
        let mut taps = super::lowpass(omega_c, win.as_slice());
        taps.truncate(num_taps);
        taps
    }

    fn compute_kaiser_beta(max_ripple: f64) -> f64 {
        // Determine Kaiser window parameters
        let ripple_db = -20.0 * max_ripple.log10();
        match ripple_db {
            x if x > 50.0 => 0.1102 * (x - 8.7),
            x if x >= 21.0 => 0.5842 * (x - 21.0).powf(0.4) + 0.07886 * (x - 21.0),
            _ => 0.0,
        }
    }

    fn design_kaiser_window(transition_bw: f64, max_ripple: f64) -> (usize, f64) {
        let beta = compute_kaiser_beta(max_ripple);
        let ripple_db = -20.0 * max_ripple.log10();
        let num_taps = (((ripple_db - 7.95) / (14.36 * transition_bw)).ceil() + 1.0) as usize;
        (num_taps, beta)
    }

    #[cfg(test)]
    #[allow(clippy::excessive_precision)]
    mod tests {
        use super::*;

        #[test]
        fn lowpass_accuracy() {
            let cutoff = 0.2;
            let transition_bw = 0.05;
            let max_ripple = 0.01;
            // Test taps generated using matlab:
            // ```
            // c = kaiserord([0.2, 0.2+0.05], [1,0], [0.01, 0.01], 1, 'cell')
            // taps = fir1(c{:});
            // ```
            let test_taps = [
                0.000801064154378,
                -0.002365829920883,
                -0.002317066829825,
                0.002912423701086,
                0.004722494338058,
                -0.002581790957417,
                -0.007902817296928,
                0.000761425035067,
                0.011472606580612,
                0.003169041375600,
                -0.014740633607712,
                -0.009778385805180,
                0.016687423513410,
                0.019601855418468,
                -0.015887002008125,
                -0.033375572621574,
                0.010135834366629,
                0.052954908730137,
                0.005241422655623,
                -0.085435542746372,
                -0.047877021123625,
                0.179797936334912,
                0.413161963225821,
                0.413161963225821,
                0.179797936334912,
                -0.047877021123625,
                -0.085435542746372,
                0.005241422655623,
                0.052954908730137,
                0.010135834366629,
                -0.033375572621574,
                -0.015887002008125,
                0.019601855418468,
                0.016687423513410,
                -0.009778385805180,
                -0.014740633607712,
                0.003169041375600,
                0.011472606580612,
                0.000761425035067,
                -0.007902817296928,
                -0.002581790957417,
                0.004722494338058,
                0.002912423701086,
                -0.002317066829825,
                -0.002365829920883,
                0.000801064154378,
            ];
            let filter_taps = lowpass::<f64>(cutoff, transition_bw, max_ripple);
            assert_eq!(filter_taps.len(), test_taps.len());
            for i in 0..filter_taps.len() {
                let tol = 1e-2;
                assert!(
                    (filter_taps[i] - test_taps[i]).abs() < tol,
                    "abs({} - {}) < {} (tap {})",
                    filter_taps[i],
                    test_taps[i],
                    tol,
                    i
                );
            }
        }

        #[test]
        fn highpass_accuracy() {
            let cutoff = 0.4;
            let transition_bw = 0.03;
            let max_ripple = 0.02;
            // Test taps generated using matlab:
            // ```
            // c = kaiserord([0.4-0.03, 0.4], [0,1], [0.02, 0.02], 1, 'cell')
            // taps = fir1(c{:});
            // ```
            let test_taps = [
                0.001101862089183,
                0.000987622783890,
                -0.003144929902063,
                0.004076732108724,
                -0.002873600914298,
                -0.000331019542578,
                0.004174826882078,
                -0.006576810746863,
                0.005795137106459,
                -0.001525908120121,
                -0.004593884000360,
                0.009497895103678,
                -0.010132234027704,
                0.005193768023735,
                0.003759912990388,
                -0.012577161166323,
                0.016278660841859,
                -0.011643406975008,
                -0.000637437587598,
                0.015485234438367,
                -0.025215481059303,
                0.023076863879726,
                -0.007061591302535,
                -0.017877268877849,
                0.040566083410670,
                -0.047438192023434,
                0.028130634522281,
                0.019450998802247,
                -0.086907962984950,
                0.157220576563611,
                -0.210275430209926,
                0.230000000000000,
                -0.210275430209926,
                0.157220576563611,
                -0.086907962984950,
                0.019450998802247,
                0.028130634522281,
                -0.047438192023434,
                0.040566083410670,
                -0.017877268877849,
                -0.007061591302535,
                0.023076863879726,
                -0.025215481059303,
                0.015485234438367,
                -0.000637437587598,
                -0.011643406975008,
                0.016278660841859,
                -0.012577161166323,
                0.003759912990388,
                0.005193768023735,
                -0.010132234027704,
                0.009497895103678,
                -0.004593884000360,
                -0.001525908120121,
                0.005795137106459,
                -0.006576810746863,
                0.004174826882078,
                -0.000331019542578,
                -0.002873600914298,
                0.004076732108724,
                -0.003144929902063,
                0.000987622783890,
                0.001101862089183,
            ];
            let filter_taps = highpass::<f64>(cutoff, transition_bw, max_ripple);
            assert_eq!(filter_taps.len(), test_taps.len());
            for i in 0..filter_taps.len() {
                let tol = 1e-2;
                assert!(
                    (filter_taps[i] - test_taps[i]).abs() < tol,
                    "abs({} - {}) < {} (tap {})",
                    filter_taps[i],
                    test_taps[i],
                    tol,
                    i
                );
            }
        }

        #[test]
        fn bandpass_accuracy() {
            let lower_cutoff = 0.2;
            let higher_cutoff = 0.4;
            let transition_bw = 0.05;
            let max_ripple = 0.02;
            // Test taps generated using matlab:
            // ```
            // c = kaiserord([0.2-0.05, 0.2, 0.4, 0.4+0.05], [0,1,0], [0.02, 0.02, 0.02], 1, 'cell')
            // taps = fir1(c{:});
            // ```
            let test_taps = [
                -0.008169897601110,
                -0.000000000000000,
                0.005286867164625,
                0.003986474461264,
                0.011611277126659,
                -0.022475033526840,
                -0.000000000000000,
                -0.013107025925601,
                0.023114960005114,
                0.027331727341472,
                -0.021725636110749,
                0.000000000000000,
                -0.075524292813853,
                0.057280413744404,
                0.029912678124411,
                0.063777614141040,
                0.000000000000000,
                -0.370383496261635,
                0.286180488307981,
                0.286180488307981,
                -0.370383496261635,
                0.000000000000000,
                0.063777614141040,
                0.029912678124411,
                0.057280413744404,
                -0.075524292813853,
                0.000000000000000,
                -0.021725636110749,
                0.027331727341472,
                0.023114960005114,
                -0.013107025925601,
                -0.000000000000000,
                -0.022475033526840,
                0.011611277126659,
                0.003986474461264,
                0.005286867164625,
                -0.000000000000000,
                -0.008169897601110,
            ];
            let filter_taps =
                bandpass::<f64>(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
            assert_eq!(filter_taps.len(), test_taps.len());
            for i in 0..filter_taps.len() {
                let tol = 1e-2;
                assert!(
                    (filter_taps[i] - test_taps[i]).abs() < tol,
                    "abs({} - {}) < {} (tap {})",
                    filter_taps[i],
                    test_taps[i],
                    tol,
                    i
                );
            }
        }

        #[test]
        fn multirate_accuracy() {
            let interp = 3;
            let decim = 2;
            let half_len = 6;
            let max_ripple = 0.0001;
            // Test taps generated using matlab:
            // ```
            // interp = 3;
            // decim = 2
            // taps = designMultirateFIR(interp,decim,6,80);
            // ```
            let test_taps = [
                0.000000000000000,
                -0.000456080632562,
                -0.001109227477145,
                0.000000000000000,
                0.004072775613512,
                0.006844614119589,
                0.000000000000000,
                -0.016512756288837,
                -0.024225080517374,
                0.000000000000000,
                0.048278505847958,
                0.066425028523671,
                0.000000000000000,
                -0.123967404911009,
                -0.172122083496355,
                0.000000000000000,
                0.395134052036115,
                0.817675050290108,
                1.000000000000000,
                0.817675050290108,
                0.395134052036115,
                0.000000000000000,
                -0.172122083496355,
                -0.123967404911009,
                0.000000000000000,
                0.066425028523671,
                0.048278505847958,
                0.000000000000000,
                -0.024225080517374,
                -0.016512756288837,
                0.000000000000000,
                0.006844614119589,
                0.004072775613512,
                0.000000000000000,
                -0.001109227477145,
                -0.000456080632562,
            ];
            let filter_taps = multirate::<f64>(interp, decim, half_len, max_ripple);
            assert_eq!(filter_taps.len(), test_taps.len());
            for i in 0..filter_taps.len() {
                let tol = 1e-5;
                assert!(
                    (filter_taps[i] - test_taps[i]).abs() < tol,
                    "abs({} - {}) < {} (tap {})",
                    filter_taps[i],
                    test_taps[i],
                    tol,
                    i
                );
            }
        }
    }
}
