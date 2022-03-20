//! Implementaions of mathematical special functions.

/// Computes the modified Bessel function of the first kind of order zero evaluated at `x`.
///
/// The computation is based on the approximation given in
/// M. Abramowitz and I. Stegun, Handbook of Mathematical Functions with Formulas, Graphs,
/// and Mathematical Tables, 1964 (eqs. 9.8.1 and 9.8.2, p. 378).
///
/// The absolute approximation error of the underlying approximation is less than
/// 1.6e-7 for `3.75 <= x <= 3.75` and less than 1.9e-7 for `3.75 <= x < ∞`.
/// However, the error is usually larger due to the numerical accuracy.
/// In practice the approximation is also reasonable for `x < -3.75`, but no
/// bounds are given.
///
/// Example usage:
/// ```
/// use futuredsp::math::special_funs;
///
/// let x = 0.34_f64;
/// let out = special_funs::besseli0(x);
/// ```
pub fn besseli0(x: f64) -> f64 {
    let t = x / 3.75;
    if x.abs() <= 3.75 {
        1.0 + 3.5156229 * t.powi(2)
            + 3.0899424 * t.powi(4)
            + 1.2067492 * t.powi(6)
            + 0.2659732 * t.powi(8)
            + 0.0360768 * t.powi(10)
            + 0.0045813 * t.powi(12)
    } else {
        if x < -3.75 {
            // The approximation is not made for this range
            warn!("Bessel approximation may be inaccurate for x < -3.75");
        }
        (x.abs().sqrt() * (-x).exp()).recip()
            * (0.39894228 + 0.01328592 * t.powi(-1) + 0.00225319 * t.powi(-2)
                - 0.00157565 * t.powi(-3)
                + 0.00916281 * t.powi(-4)
                - 0.02057706 * t.powi(-5)
                + 0.02635537 * t.powi(-6)
                - 0.01647633 * t.powi(-7)
                + 0.00392377 * t.powi(-8))
    }
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod tests {
    use super::*;

    #[test]
    fn besseli0_accuracy() {
        // -3.75 <= x <= 3.75
        let abs_epsilon = 1.6e-7;
        let rel_epsilon = 1.0e-7;
        let input_x = [
            -3.75_f64, -3.0, -2.0, -1.5, -1.0, -0.3, -0.2, -0.1, -0.01, -0.001, 0.0, 0.001, 0.01,
            0.1, 0.2, 0.3, 1.0, 1.5, 2.0, 3.0, 3.75,
        ];
        let test_x = [
            9.118945860844564_f64,
            4.880792585865025,
            2.279585302336067,
            1.646723189772891,
            1.266065877752008,
            1.022626879351597,
            1.010025027795146,
            1.002501562934095,
            1.000025000156250,
            1.000000250000016,
            1.000000000000000,
            1.000000250000016,
            1.000025000156250,
            1.002501562934095,
            1.010025027795146,
            1.022626879351597,
            1.266065877752008,
            1.646723189772891,
            2.279585302336067,
            4.880792585865025,
            9.118945860844564,
        ]; // Computed using MATLAB besseli()
        for i in 0..input_x.len() {
            let tol = abs_epsilon + rel_epsilon * test_x[i].abs();
            assert!(
                (besseli0(input_x[i]) - test_x[i]).abs() < tol,
                "abs({} - {}) < {} (x={})",
                besseli0(input_x[i]),
                test_x[i],
                tol,
                input_x[i],
            );
        }
        // 3.75 < x < infinity
        let abs_epsilon = 1.9e-7;
        let rel_epsilon = 1.0e-7;
        let input_x = [3.8_f64, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0, 9.0, 10.0, 20.0];
        let test_x = [
            9.516888026098954_f64,
            11.301921952136331,
            17.481171855609279,
            27.239871823604449,
            42.694645151847787,
            67.234406976477985,
            1.685939085102897e2,
            4.275641157218048e2,
            1.093588354511375e3,
            2.815716628466255e3,
            4.355828255955355e7,
        ]; // Computed using MATLAB besseli()
        for i in 0..input_x.len() {
            let tol = abs_epsilon + rel_epsilon * test_x[i].abs();
            assert!(
                (besseli0(input_x[i]) - test_x[i]).abs() < tol,
                "abs({} - {}) < {} (x={})",
                besseli0(input_x[i]),
                test_x[i],
                tol,
                input_x[i],
            );
        }
    }
}
