use rustfft::num_complex::Complex;
use rustfft::{self, FftPlanner};
use std::cmp;
use std::mem::size_of;
use std::sync::Arc;

use crate::anyhow::Result;
use crate::runtime::Kernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Fft {
    plan: Arc<dyn rustfft::Fft<f32>>,
    scratch: [Complex<f32>; 2048 * 10],
}

impl Fft {
    pub fn new() -> Block {
        let mut planner = FftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(2048);

        Block::new(
            BlockMetaBuilder::new("Fft").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<Complex<f32>>())
                .add_output("out", size_of::<Complex<f32>>())
                .build(),
            MessageIoBuilder::<Fft>::new().build(),
            Fft {
                plan,
                scratch: [Complex::new(0.0, 0.0); 2048 * 10],
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
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<Complex<f32>>();

        let m = cmp::min(i.len(), o.len());
        let n = (m / 2048) * 2048;

        if sio.input(0).finished() {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        self.plan
            .process_outofplace_with_scratch(&mut i[0..n], &mut o[0..n], &mut self.scratch);

        sio.input(0).consume(n);
        sio.output(0).produce(n);

        Ok(())
    }
}

/// Computes a FFT
///
/// This block computes the FFT on 2048 samples at a time, outputting 2048 samples per FFT.
///
/// # Inputs
///
/// `in`: Input samples
///
/// # Outputs
///
/// `out`: FFT results
///
/// # Usage
/// ```
/// use futuresdr::blocks::FftBuilder;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let fft = fg.add_block(FftBuilder::new().build());
/// ```
pub struct FftBuilder {}

impl FftBuilder {
    pub fn new() -> FftBuilder {
        FftBuilder {}
    }

    pub fn build(self) -> Block {
        Fft::new()
    }
}

impl Default for FftBuilder {
    fn default() -> Self {
        Self::new()
    }
}
