use rustfft::num_complex::Complex32;
use rustfft::FftPlanner;
use std::cmp;
use std::sync::Arc;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

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
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let fft = fg.add_block(Fft::new(2048));
/// ```
pub struct Fft {
    len: usize,
    fft_shift: bool,
    direction: FftDirection,
    normalize: Option<f32>,
    plan: Arc<dyn rustfft::Fft<f32>>,
    scratch: Box<[Complex32]>,
}

/// Fft direction.
pub enum FftDirection {
    /// Forward
    Forward,
    /// Inverse
    Inverse,
}

impl Fft {
    /// Create FFT block
    pub fn new(len: usize) -> TypedBlock<Self> {
        Self::with_direction(len, FftDirection::Forward)
    }
    /// Create FFT block with [`FftDirection`]
    pub fn with_direction(len: usize, direction: FftDirection) -> TypedBlock<Self> {
        Self::with_options(len, direction, false, None)
    }
    /// Create FFT block with options (direction, shift, normalization)
    pub fn with_options(
        len: usize,
        direction: FftDirection,
        fft_shift: bool,
        normalize: Option<f32>,
    ) -> TypedBlock<Self> {
        let mut planner = FftPlanner::<f32>::new();
        let plan = match direction {
            FftDirection::Forward => planner.plan_fft_forward(len),
            FftDirection::Inverse => planner.plan_fft_inverse(len),
        };

        TypedBlock::new(
            BlockMetaBuilder::new("Fft").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::<Fft>::new()
                .add_input("fft_size", Self::fft_size_handler)
                .build(),
            Fft {
                len,
                plan,
                direction,
                fft_shift,
                normalize,
                scratch: vec![Complex32::new(0.0, 0.0); len * 10].into_boxed_slice(),
            },
        )
    }

    /// Handle incoming messages to change FFT size
    #[message_handler]
    fn fft_size_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
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
impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = unsafe { sio.input(0).slice_mut::<Complex32>() };
        let o = sio.output(0).slice::<Complex32>();

        let m = cmp::min(i.len(), o.len());
        let m = (m / self.len) * self.len;

        if m > 0 {
            if matches!(self.direction, FftDirection::Inverse) && self.fft_shift {
                for f in 0..(m / self.len) {
                    let mut sym = vec![Complex32::new(0.0, 0.0); self.len];
                    sym.copy_from_slice(&i[f * self.len..(f + 1) * self.len]);
                    for k in 0..self.len {
                        i[f * self.len + k] = sym[(k + self.len / 2) % self.len]
                    }
                }
            }

            self.plan.process_outofplace_with_scratch(
                &mut i[0..m],
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

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == (m / self.len) * self.len {
            io.finished = true;
        }

        Ok(())
    }
}
