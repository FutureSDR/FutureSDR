use crate::runtime::dev::prelude::*;

/// Apply a function to received samples.
///
/// # Inputs
///
/// `in` Input Samples.
///
/// # Outputs
///
/// No Outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::Sink;
///
/// let sink = Sink::new(|x: &f32| println!("{}", x));
/// ```
#[derive(Block)]
pub struct Sink<F, A, I = DefaultCpuReader<A>>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
    I: CpuBufferReader<Item = A>,
{
    #[input]
    input: I,
    f: F,
}

impl<F, A> Sink<F, A, DefaultCpuReader<A>>
where
    F: FnMut(&A) + Send + 'static,
    A: CpuSample,
{
    /// Create Sink block with the default stream buffer.
    pub fn new(f: F) -> Self {
        Self::with_buffer(f)
    }
}

impl<F, A, I> Sink<F, A, I>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
    I: CpuBufferReader<Item = A>,
{
    /// Create Sink block with a custom stream buffer.
    pub fn with_buffer(f: F) -> Self {
        Self {
            input: I::default(),
            f,
        }
    }
}

#[doc(hidden)]
impl<F, A, I> Kernel for Sink<F, A, I>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
    I: CpuBufferReader<Item = A>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        for v in i.iter() {
            (self.f)(v);
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(i_len);

        Ok(())
    }
}
