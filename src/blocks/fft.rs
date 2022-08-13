use rustfft::num_complex::Complex;
use rustfft::{self, FftPlanner};
use std::cmp;
use std::mem::size_of;
use std::sync::Arc;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Computes an FFT
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
    plan: Arc<dyn rustfft::Fft<f32>>,
    scratch: Box<[Complex<f32>]>,
}

impl Fft {
    pub fn new(len: usize) -> Block {
        let mut planner = FftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(len);

        Block::new(
            BlockMetaBuilder::new("Fft").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<Complex<f32>>())
                .add_output("out", size_of::<Complex<f32>>())
                .build(),
            MessageIoBuilder::<Fft>::new().build(),
            Fft {
                len,
                plan,
                scratch: vec![Complex::new(0.0, 0.0); len * 10].into_boxed_slice(),
            },
        )
    }
}

#[async_trait]
impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = unsafe { sio.input(0).slice_mut::<Complex<f32>>() };
        let o = sio.output(0).slice::<Complex<f32>>();

        let m = cmp::min(i.len(), o.len());
        let m = (m / self.len) * self.len;

        if m > 0 {
            self.plan.process_outofplace_with_scratch(
                &mut i[0..m],
                &mut o[0..m],
                &mut self.scratch,
            );

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == (m / self.len) * self.len {
            io.finished = true;
        }

        Ok(())
    }
}
