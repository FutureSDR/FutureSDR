//! A collection of filter window functions.

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
