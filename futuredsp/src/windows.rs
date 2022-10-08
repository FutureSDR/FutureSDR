//! A collection of window functions.

extern crate alloc;
use crate::math::special_funs;
use alloc::vec::Vec;

/// A rectangular window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::rect(64);
/// ```
pub fn rect(len: usize) -> Vec<f64> {
    vec![1.0; len]
}

/// A Bartlett window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::bartlett(38);
/// ```
pub fn bartlett(len: usize) -> Vec<f64> {
    let alpha = (len - 1) as f64 / 2.0;
    (0..len)
        .map(|n| match n as f64 {
            n if n < alpha => n / alpha,
            n => 2.0 - n / alpha,
        })
        .collect()
}
/// A generalized cosine window of a given length with coefficients `coeffs`.
/// If `periodic` is `false`, a symmetric filter is returned, which is suitable for
/// filter design. If `periodic` is true, a perfect periodic window is
/// returned, which is useful for spectral analysis. The periodic window is generated
/// by computing a window of length `len+1` and then truncating it to the first `len` taps.
///
/// The generalized cosine window is on the form:
///```text
/// w[n] = sum_k (-1)^k * coeffs[k] * cos(2*π*k*n/N),     0 ≤ n ≤ N.
///```
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::gen_cos(38, &[0.1, 0.2], false);
/// ```
pub fn gen_cos(len: usize, coeffs: &[f64], periodic: bool) -> Vec<f64> {
    let (len, truncate) = match periodic {
        true => (len + 1, true),
        false => (len, false),
    };
    let alpha = (len - 1) as f64 / 2.0;
    let mut taps: Vec<f64> = (0..len)
        .map(|n| {
            (0..coeffs.len())
                .map(|k| {
                    (-1.0f64).powi(k as i32)
                        * coeffs[k]
                        * (core::f64::consts::PI * ((k * n) as f64) / alpha).cos()
                })
                .sum()
        })
        .collect();
    if truncate {
        taps.remove(len);
    }
    taps
}

/// A Blackman window of a given length. If `periodic` is `true` a periodic
/// window is returned, otherwise a symmetric window. See [`gen_cos`] for more details.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::blackman(38, false);
/// ```
pub fn blackman(len: usize, periodic: bool) -> Vec<f64> {
    gen_cos(len, &[0.42, 0.5, 0.08], periodic)
}

/// A Hamming window of a given length. If `periodic` is `true` a periodic
/// window is returned, otherwise a symmetric window. See [`gen_cos`] for more details.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::hamming(38, false);
/// ```
pub fn hamming(len: usize, periodic: bool) -> Vec<f64> {
    gen_cos(len, &[0.54, 0.46], periodic)
}

/// A Hann window of a given length (sometimes also referred to as Hanning window).
/// If `periodic` is `true` a periodic window is returned, otherwise a symmetric window.
/// See [`gen_cos`] for more details.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::hann(38, false);
/// ```
pub fn hann(len: usize, periodic: bool) -> Vec<f64> {
    gen_cos(len, &[0.5, 0.5], periodic)
}

/// A Kaiser window of a given length and shape parameter.
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::kaiser(38, 0.5);
/// ```
pub fn kaiser(len: usize, beta: f64) -> Vec<f64> {
    let alpha = (len - 1) as f64 / 2.0;
    (0..len)
        .map(|n| {
            let x = beta * (1.0 - ((n as f64 - alpha) / alpha).powi(2)).sqrt();
            special_funs::besseli0(x) / special_funs::besseli0(beta)
        })
        .collect()
}

/// A Gaussian window of a given length with width factor `alpha`, which is
/// inversely proportional to the standard deviation.
/// Note that sometimes, the width of a Gaussian window is specified in terms of
/// a parameter that is proportional to the standard deviation (inversely proportional to alpha).
///
/// Example usage:
/// ```
/// use futuredsp::windows;
///
/// let taps = windows::gaussian(38, 2.5);
/// ```
pub fn gaussian(len: usize, alpha: f64) -> Vec<f64> {
    let mid = ((len - 1) as f64) / 2.0;
    let std_dev = mid / alpha;
    (0..len)
        .map(|n| (-(n as f64 - mid).powi(2) / (2.0 * std_dev.powi(2))).exp())
        .collect()
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod tests {
    use super::*;

    #[test]
    fn bartlett_accuracy() {
        let n_taps = 38;
        let test_taps = [
            0.000000000000000,
            0.054054054054054,
            0.108108108108108,
            0.162162162162162,
            0.216216216216216,
            0.270270270270270,
            0.324324324324324,
            0.378378378378378,
            0.432432432432432,
            0.486486486486487,
            0.540540540540541,
            0.594594594594595,
            0.648648648648649,
            0.702702702702703,
            0.756756756756757,
            0.810810810810811,
            0.864864864864865,
            0.918918918918919,
            0.972972972972973,
            0.972972972972973,
            0.918918918918919,
            0.864864864864865,
            0.810810810810811,
            0.756756756756757,
            0.702702702702703,
            0.648648648648649,
            0.594594594594595,
            0.540540540540541,
            0.486486486486487,
            0.432432432432432,
            0.378378378378378,
            0.324324324324324,
            0.270270270270270,
            0.216216216216216,
            0.162162162162162,
            0.108108108108108,
            0.054054054054054,
            0.000000000000000,
        ]; // Computed using MATLAB bartlett()
        let window = bartlett(n_taps);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }

    #[test]
    fn blackman_accuracy() {
        let n_taps = 38;
        let test_taps = [
            0.000000000000000,
            0.002622240463032,
            0.010804137614933,
            0.025437526103984,
            0.047836464440438,
            0.079501212725209,
            0.121830058635970,
            0.175815273593484,
            0.241762085771086,
            0.319067599318524,
            0.406090274118759,
            0.500130613698768,
            0.597531218304494,
            0.693890766450019,
            0.784373343232954,
            0.864083342844346,
            0.928468223065274,
            0.973707602519644,
            0.997048017099080,
            0.997048017099080,
            0.973707602519644,
            0.928468223065274,
            0.864083342844346,
            0.784373343232954,
            0.693890766450019,
            0.597531218304494,
            0.500130613698768,
            0.406090274118759,
            0.319067599318524,
            0.241762085771086,
            0.175815273593484,
            0.121830058635970,
            0.079501212725209,
            0.047836464440438,
            0.025437526103984,
            0.010804137614933,
            0.002622240463032,
            0.000000000000000,
        ]; // Computed using MATLAB blackman()
        let window = blackman(n_taps, false);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }

    #[test]
    fn gaussian_accuracy() {
        let n_taps = 38;
        let alpha = 2.2;
        let test_taps = [
            0.088921617459386,
            0.114698396715108,
            0.145869896370133,
            0.182907845975514,
            0.226129555908018,
            0.275638995194989,
            0.331270161955420,
            0.392538551605780,
            0.458606963743581,
            0.528271654134153,
            0.599973826254896,
            0.671839656444629,
            0.741749563359992,
            0.807434491048862,
            0.866593901972413,
            0.917027361308578,
            0.956769434838275,
            0.984216463455208,
            0.998233847825964,
            0.998233847825964,
            0.984216463455208,
            0.956769434838275,
            0.917027361308578,
            0.866593901972413,
            0.807434491048862,
            0.741749563359992,
            0.671839656444629,
            0.599973826254896,
            0.528271654134153,
            0.458606963743581,
            0.392538551605780,
            0.331270161955420,
            0.275638995194989,
            0.226129555908018,
            0.182907845975514,
            0.145869896370133,
            0.114698396715108,
            0.088921617459386,
        ]; // Computed using MATLAB gausswin()
        let window = gaussian(n_taps, alpha);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }

    #[test]
    fn hamming_accuracy() {
        let n_taps = 38;
        let test_taps = [
            0.080000000000000,
            0.086616681240054,
            0.106276375087901,
            0.138413507945853,
            0.182103553013518,
            0.236089627240563,
            0.298818649563673,
            0.368486020221058,
            0.443087535801966,
            0.520477046529772,
            0.598428197083564,
            0.674698474787213,
            0.747093722616130,
            0.813531261099972,
            0.872099803219081,
            0.921114438652167,
            0.959165105578543,
            0.985157155589410,
            0.998342844729562,
            0.998342844729562,
            0.985157155589410,
            0.959165105578543,
            0.921114438652167,
            0.872099803219081,
            0.813531261099972,
            0.747093722616130,
            0.674698474787213,
            0.598428197083564,
            0.520477046529772,
            0.443087535801966,
            0.368486020221058,
            0.298818649563673,
            0.236089627240563,
            0.182103553013518,
            0.138413507945853,
            0.106276375087901,
            0.086616681240054,
            0.080000000000000,
        ]; // Computed using MATLAB hamming()
        let window = hamming(n_taps, false);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }

    #[test]
    fn hann_accuracy() {
        let n_taps = 38;
        let test_taps = [
            0.000000000000000,
            0.007192044826146,
            0.028561277269458,
            0.063492943419406,
            0.110982122840780,
            0.169662638304959,
            0.237846358221384,
            0.313571761109846,
            0.394660365002137,
            0.478779398401926,
            0.563508909873439,
            0.646411385638275,
            0.725101872408837,
            0.797316588152143,
            0.860978046977262,
            0.914254824621921,
            0.955614245194068,
            0.983866473466749,
            0.998198744271263,
            0.998198744271263,
            0.983866473466749,
            0.955614245194068,
            0.914254824621921,
            0.860978046977262,
            0.797316588152143,
            0.725101872408837,
            0.646411385638275,
            0.563508909873439,
            0.478779398401926,
            0.394660365002137,
            0.313571761109846,
            0.237846358221384,
            0.169662638304959,
            0.110982122840780,
            0.063492943419406,
            0.028561277269458,
            0.007192044826146,
            0.000000000000000,
        ]; // Computed using MATLAB hann()
        let window = hann(n_taps, false);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }

    #[test]
    fn kaiser_accuracy() {
        let beta = 5.653;
        let n_taps = 38;
        let test_taps = [
            0.020392806629217,
            0.041484435695145,
            0.070067692203354,
            0.106749242190360,
            0.151823492501156,
            0.205218380642171,
            0.266458522450125,
            0.334649288647039,
            0.408484172820245,
            0.486276388059038,
            0.566014081873242,
            0.645436995269608,
            0.722130922112194,
            0.793635055125124,
            0.857556328958361,
            0.911684263160396,
            0.954099618076827,
            0.983270424870408,
            0.998129626296050,
            0.998129626296050,
            0.983270424870408,
            0.954099618076827,
            0.911684263160396,
            0.857556328958361,
            0.793635055125124,
            0.722130922112194,
            0.645436995269608,
            0.566014081873242,
            0.486276388059038,
            0.408484172820245,
            0.334649288647039,
            0.266458522450125,
            0.205218380642171,
            0.151823492501156,
            0.106749242190360,
            0.070067692203354,
            0.041484435695145,
            0.020392806629217,
        ]; // Computed using MATLAB kaiser()
        let window = kaiser(n_taps, beta);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window[i] - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window[i],
                tap,
                tol,
                i
            );
        }
    }
}
