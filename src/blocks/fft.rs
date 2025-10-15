use futuresdr::prelude::*;
use rustfft::FftPlanner;
use std::cmp;
use std::sync::Arc;

/// Compute an FFT.
///
/// This block computes the FFT on `len` samples at a time, outputting `len` samples per FFT.
///
/// # Inputs
///
/// `in`: Input samples (Complex32)
///
/// # Outputs
///
/// `out`: FFT results (Complex32)
///
/// # Messages
///
/// `fft_size`: Change the FFT size (Pmt::Usize)
///
/// # Usage
/// ```
/// use futuresdr::blocks::Fft;
///
/// let fft: Fft<> = Fft::new(2048);
/// ```
#[derive(Block)]
#[message_inputs(fft_size)]
pub struct Fft<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    len: usize,
    fft_shift: bool,
    direction: FftDirection,
    normalize: Option<f32>,
    plan: Arc<dyn rustfft::Fft<f32>>,
    buff: Box<[Complex32]>,
    scratch: Box<[Complex32]>,
}

/// Fft direction.
pub enum FftDirection {
    /// Forward
    Forward,
    /// Inverse
    Inverse,
}

const BUFF_FFTS: usize = 32;

impl<I, O> Fft<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create FFT block
    pub fn new(len: usize) -> Self {
        Self::with_direction(len, FftDirection::Forward)
    }
    /// Create FFT block with [`FftDirection`]
    pub fn with_direction(len: usize, direction: FftDirection) -> Self {
        Self::with_options(len, direction, false, None)
    }
    /// Create FFT block with options (direction, shift, normalization)
    pub fn with_options(
        len: usize,
        direction: FftDirection,
        fft_shift: bool,
        normalize: Option<f32>,
    ) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let plan = match direction {
            FftDirection::Forward => planner.plan_fft_forward(len),
            FftDirection::Inverse => planner.plan_fft_inverse(len),
        };
        let scratch_size = plan.get_outofplace_scratch_len();

        let mut input = I::default();
        input.set_min_items(len);
        let mut output = O::default();
        output.set_min_items(len);

        Self {
            input,
            output,
            len,
            plan,
            direction,
            fft_shift,
            normalize,
            buff: vec![Complex32::new(0.0, 0.0); len * BUFF_FFTS].into_boxed_slice(),
            scratch: vec![Complex32::new(0.0, 0.0); scratch_size].into_boxed_slice(),
        }
    }

    /// Handle incoming messages to change FFT size
    async fn fft_size(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Usize(new_len) => self.set_fft_size(new_len),
            Pmt::Null => Ok(Pmt::Usize(self.len)),
            _ => Ok(Pmt::InvalidValue),
        }
    }

    /// Set a new FFT size
    fn set_fft_size(&mut self, new_len: usize) -> Result<Pmt> {
        let mut planner = FftPlanner::<f32>::new();
        let new_plan = match self.direction {
            FftDirection::Forward => planner.plan_fft_forward(new_len),
            FftDirection::Inverse => planner.plan_fft_inverse(new_len),
        };

        self.len = new_len;
        self.plan = new_plan;
        self.scratch = vec![Complex32::new(0.0, 0.0); new_len * 10].into_boxed_slice();

        Ok(Pmt::Ok)
    }
}

#[doc(hidden)]
impl<I, O> Kernel for Fft<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (i, in_tags) = self.input.slice_with_tags();
        let (o, mut out_tags) = self.output.slice_with_tags();

        let m = cmp::min(i.len(), o.len());
        let m = (m / self.len) * self.len;
        let m = cmp::min(m, self.len * BUFF_FFTS);

        if m > 0 {
            in_tags
                .iter()
                .filter(|t| t.index < m)
                .for_each(|t| out_tags.add_tag(t.index, t.tag.clone()));

            if matches!(self.direction, FftDirection::Inverse) && self.fft_shift {
                for f in 0..(m / self.len) {
                    for k in 0..self.len {
                        self.buff[f * self.len + k] =
                            i[f * self.len + ((k + self.len / 2) % self.len)]
                    }
                }
            } else {
                self.buff[0..m].copy_from_slice(&i[0..m]);
            }

            self.plan.process_outofplace_with_scratch(
                &mut self.buff[0..m],
                &mut o[0..m],
                &mut self.scratch,
            );

            if matches!(self.direction, FftDirection::Forward) && self.fft_shift {
                for f in 0..(m / self.len) {
                    let mut sym = vec![Complex32::new(0.0, 0.0); self.len];
                    sym.copy_from_slice(&o[f * self.len..(f + 1) * self.len]);
                    for k in 0..self.len {
                        o[f * self.len + k] = sym[(k + self.len / 2) % self.len]
                    }
                }
            }

            if let Some(fac) = self.normalize {
                for item in o[0..m].iter_mut() {
                    *item *= fac;
                }
            }

            self.input.consume(m);
            self.output.produce(m);
        }

        if self.input.finished() && m == (m / self.len) * self.len {
            io.finished = true;
        }

        Ok(())
    }
}
