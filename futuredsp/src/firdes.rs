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
            warn!("num_taps must be odd. Adding one.");
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
    use crate::windows::KaiserWindow;
    use alloc::vec::Vec;

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
    /// let cutoff = 2_000.0 / sampling_freq as f32;
    /// let transition_bw = 500.0 / sampling_freq as f32;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::lowpass(cutoff, transition_bw, max_ripple);
    /// ```
    pub fn lowpass(cutoff: f32, transition_bw: f32, max_ripple: f32) -> Vec<f32> {
        assert!(cutoff > 0.0, "cutoff must be greater than 0");
        assert!(transition_bw > 0.0, "transition_bw must be greater than 0");
        assert!(
            cutoff + transition_bw < 1.0 / 2.0,
            "cutoff+transition_bw must be less than 1/2"
        );
        let (num_taps, beta) = design_kaiser_window(transition_bw, max_ripple);
        let win = KaiserWindow::new(num_taps, beta);
        let omega_c = (2.0 * cutoff + transition_bw) / 2.0;
        super::lowpass(num_taps, omega_c, win)
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
    /// let cutoff = 4_000.0 / sampling_freq as f32;
    /// let transition_bw = 500.0 / sampling_freq as f32;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::highpass(cutoff, transition_bw, max_ripple);
    /// ```
    pub fn highpass(cutoff: f32, transition_bw: f32, max_ripple: f32) -> Vec<f32> {
        assert!(cutoff > 0.0, "cutoff must be greater than 0");
        assert!(transition_bw > 0.0, "transition_bw must be greater than 0");
        assert!(
            cutoff + transition_bw < 1.0 / 2.0,
            "cutoff+transition_bw must be less than 1/2"
        );
        // Determine cutoff frequency of the underlying ideal lowpass filter
        let (num_taps, beta) = design_kaiser_window(transition_bw, max_ripple);
        // Number of taps must be odd
        let num_taps = num_taps + (num_taps % 2);
        let win = KaiserWindow::new(num_taps, beta);
        let omega_c = (2.0 * cutoff - transition_bw) / 2.0;
        super::highpass(num_taps, omega_c, win)
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
    /// let lower_cutoff = 1_000.0 / sampling_freq as f32;
    /// let higher_cutoff = 4_000.0 / sampling_freq as f32;
    /// let transition_bw = 500.0 / sampling_freq as f32;
    /// let max_ripple = 0.001;
    /// let taps = firdes::kaiser::bandpass(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
    /// ```
    pub fn bandpass(
        lower_cutoff: f32,
        higher_cutoff: f32,
        transition_bw: f32,
        max_ripple: f32,
    ) -> Vec<f32> {
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
        let win = KaiserWindow::new(num_taps, beta);
        let lower_omega_c = (2.0 * lower_cutoff - transition_bw) / 2.0;
        let higher_omega_c = (2.0 * higher_cutoff + transition_bw) / 2.0;
        super::bandpass(num_taps, lower_omega_c, higher_omega_c, win)
    }

    fn design_kaiser_window(transition_bw: f32, max_ripple: f32) -> (usize, f32) {
        // Determine Kaiser window parameters
        let ripple_db = -20.0 * max_ripple.log10();
        let beta = match ripple_db {
            x if x > 50.0 => 0.1102 * (x - 8.7),
            x if x >= 21.0 => 0.5842 * (x - 21.0).powf(0.4) + 0.07886 * (x - 21.0),
            _ => 0.0,
        };
        let num_taps = (((ripple_db - 7.95) / (14.36 * transition_bw)).ceil() + 1.0) as usize;
        (num_taps, beta)
    }

    #[cfg(test)]
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
            let filter_taps = lowpass(cutoff, transition_bw, max_ripple);
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
            let filter_taps = highpass(cutoff, transition_bw, max_ripple);
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
            let filter_taps = bandpass(lower_cutoff, higher_cutoff, transition_bw, max_ripple);
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
}
