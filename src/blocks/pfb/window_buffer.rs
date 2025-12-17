use num_complex::Complex32;

#[derive(Clone, Debug)]
pub struct WindowBuffer {
    buffer_len: usize,
    circular_buffer: Vec<Complex32>,
    start_idx: usize,
    num_samples_missing: usize,
}

impl WindowBuffer {
    /// Create Circular Window Buffer
    pub fn new(buffer_len: usize, pad_start: bool) -> WindowBuffer {
        WindowBuffer {
            buffer_len,
            circular_buffer: vec![Complex32::default(); buffer_len * 2],
            start_idx: 0,
            num_samples_missing: if pad_start { 0 } else { buffer_len },
        }
    }

    /// add a new sample at the end of the window, dropping the oldest one if the window is already filled
    pub fn push(&mut self, sample: Complex32) {
        self.circular_buffer[(self.start_idx as isize - self.num_samples_missing as isize)
            .rem_euclid(self.buffer_len as isize) as usize] = sample;
        self.circular_buffer[(self.start_idx as isize - self.num_samples_missing as isize)
            .rem_euclid(self.buffer_len as isize) as usize
            + self.buffer_len] = sample;
        self.num_samples_missing = self.num_samples_missing.saturating_sub(1);
        self.start_idx += 1;
        self.start_idx %= self.buffer_len;
    }

    /// access the window as a contiguous slice
    pub fn get_as_slice(&self) -> &[Complex32] {
        debug_assert_eq!(self.num_samples_missing, 0);
        &self.circular_buffer
            [self.start_idx..self.start_idx + self.buffer_len - self.num_samples_missing]
    }

    pub fn filled(&self) -> bool {
        self.num_samples_missing == 0
    }
}
