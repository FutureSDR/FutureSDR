//! A collection of filter window functions.

use crate::math::special_funs;

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
        for i in 0..n_taps {
            let tol = 1e-5;
            assert!(
                (window.get(i) - test_taps[i]).abs() < tol,
                "abs({} - {}) < {} (tap {})",
                window.get(i),
                test_taps[i],
                tol,
                i
            );
        }
    }
}
