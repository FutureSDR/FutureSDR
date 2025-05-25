use crate::prelude::*;

/// Drop samples.
///
/// # Inputs
///
/// `in`: Stream to drop
///
/// # Outputs
///
/// No outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::NullSink;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(NullSink::<Complex<f32>>::new());
/// ```
#[derive(Block)]
pub struct NullSink<T: CpuSample, I: CpuBufferReader<Item = T> = DefaultCpuReader<T>> {
    n_received: usize,
    #[input]
    input: I,
}

impl<T, I> NullSink<T, I>
where
    T: CpuSample,
    I: CpuBufferReader<Item = T>,
{
    /// Create NullSink block
    pub fn new() -> Self {
        Self {
            n_received: 0,
            input: I::default(),
        }
    }
    /// Get number of received samples
    pub fn n_received(&self) -> usize {
        self.n_received
    }
}

impl<T, I> Default for NullSink<T, I>
where
    T: CpuSample,
    I: CpuBufferReader<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T, I> Kernel for NullSink<T, I>
where
    T: CpuSample,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let n = self.input().slice().len();
        if n > 0 {
            self.n_received += n;
            self.input().consume(n);
        }

        if self.input().finished() {
            io.finished = true;
        }

        Ok(())
    }
}
