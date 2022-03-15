//! A collection of filter window functions.

use crate::math::{consts, special_funs};

/// A generic trait for filter window functions.
pub trait FilterWindow<TapsType> {
    /// Returns the window function at index `index`
    fn get(&self, index: usize) -> TapsType;
}

/// A rectangular window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, RectWindow};
///
/// let window = RectWindow::new(64);
/// let tap0 = window.get(0);
/// ```
pub struct RectWindow {
    num_taps: usize,
}

impl RectWindow {
    /// Create a new rectangular window with `num_taps` taps.
    pub fn new(num_taps: usize) -> Self {
        Self { num_taps }
    }
}
impl FilterWindow<f32> for RectWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        1.0
    }
}

/// A Bartlett window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, BartlettWindow};
///
/// let window = BartlettWindow::new(38);
/// let tap0 = window.get(0);
/// ```
pub struct BartlettWindow {
    num_taps: usize,
}

impl BartlettWindow {
    /// Create a new Bartlett window with `num_taps` taps.
    pub fn new(num_taps: usize) -> Self {
        Self { num_taps }
    }
}
impl FilterWindow<f32> for BartlettWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let alpha = (self.num_taps - 1) as f32 / 2.0;
        match (index as f32) < alpha {
            true => (index as f32) / alpha,
            false => 2.0 - (index as f32) / alpha,
        }
    }
}

/// A Blackman window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, BlackmanWindow};
///
/// let window = BlackmanWindow::new(38);
/// let tap0 = window.get(0);
/// ```
pub struct BlackmanWindow {
    num_taps: usize,
}

impl BlackmanWindow {
    /// Create a new Blackman window with `num_taps` taps.
    pub fn new(num_taps: usize) -> Self {
        Self { num_taps }
    }
}
impl FilterWindow<f32> for BlackmanWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let alpha = (self.num_taps - 1) as f32 / 2.0;
        0.42 - 0.5 * (consts::f32::PI * (index as f32) / alpha).cos()
            + 0.08 * (2.0 * consts::f32::PI * (index as f32) / alpha).cos()
    }
}

/// A Gaussian window of a given length with width factor `alpha`, which is
/// inversely proportional to the standard deviation.
/// Note that sometimes, the width of a Gaussian window is specified in terms of
/// a parameter that is proportional to the standard deviation (inversely proportional to alpha).
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, GaussianWindow};
///
/// let window = GaussianWindow::new(38, 2.5);
/// let tap0 = window.get(0);
/// ```
pub struct GaussianWindow {
    num_taps: usize,
    alpha: f32,
}

impl GaussianWindow {
    /// Create a new Blackman window with `num_taps` taps and width factor `alpha`.
    pub fn new(num_taps: usize, alpha: f32) -> Self {
        Self { num_taps, alpha }
    }
}
impl FilterWindow<f32> for GaussianWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let mid = ((self.num_taps - 1) as f32) / 2.0;
        let std_dev = mid / self.alpha;
        let n = index as f32 - mid;
        (-n.powi(2) / (2.0 * std_dev.powi(2))).exp()
    }
}

/// A Hamming window of a given length.
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, HammingWindow};
///
/// let window = HammingWindow::new(38);
/// let tap0 = window.get(0);
/// ```
pub struct HammingWindow {
    num_taps: usize,
}

impl HammingWindow {
    /// Create a new Hamming window with `num_taps` taps.
    pub fn new(num_taps: usize) -> Self {
        Self { num_taps }
    }
}
impl FilterWindow<f32> for HammingWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let alpha = (self.num_taps - 1) as f32 / 2.0;
        0.54 - 0.46 * (consts::f32::PI * (index as f32) / alpha).cos()
    }
}

/// A Hann window of a given length (sometimes also referred to as Hanning window).
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, HannWindow};
///
/// let window = HannWindow::new(38);
/// let tap0 = window.get(0);
/// ```
pub struct HannWindow {
    num_taps: usize,
}

impl HannWindow {
    /// Create a new Hann window with `num_taps` taps.
    pub fn new(num_taps: usize) -> Self {
        Self { num_taps }
    }
}
impl FilterWindow<f32> for HannWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let alpha = (self.num_taps - 1) as f32 / 2.0;
        0.5 * (1.0 - (consts::f32::PI * (index as f32) / alpha).cos())
    }
}

/// A Kaiser window of a given length and shape parameter.
///
/// Example usage:
/// ```
/// use futuredsp::windows::{FilterWindow, KaiserWindow};
///
/// let window = KaiserWindow::new(38, 0.5);
/// let tap0 = window.get(0);
/// ```
pub struct KaiserWindow {
    num_taps: usize,
    beta: f32,
}

impl KaiserWindow {
    /// Create a new Kaiser window with `num_taps` taps and shape `beta`.
    pub fn new(num_taps: usize, beta: f32) -> Self {
        Self { num_taps, beta }
    }
}
impl FilterWindow<f32> for KaiserWindow {
    fn get(&self, index: usize) -> f32 {
        if index >= self.num_taps {
            return 0.0;
        }
        let alpha = (self.num_taps - 1) as f32 / 2.0;
        let x = self.beta * (1.0 - ((index as f32 - alpha) / alpha).powi(2)).sqrt();
        (special_funs::besseli0(x as f64) / special_funs::besseli0(self.beta as f64)) as f32
    }
}

#[cfg(test)]
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
        let window = BartlettWindow::new(n_taps);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
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
        let window = BlackmanWindow::new(n_taps);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
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
        let window = GaussianWindow::new(n_taps, alpha);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
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
        let window = HammingWindow::new(n_taps);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
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
        let window = HannWindow::new(n_taps);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
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
        let window = KaiserWindow::new(n_taps, beta);
        for (i, tap) in test_taps.iter().enumerate() {
            let tol = 1e-5;
            assert!(
                (window.get(i) - tap).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
                tap,
                tol,
                i
            );
        }
    }
}
