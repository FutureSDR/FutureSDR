use crate::prelude::*;

/// Apply a function to split a stream.
#[derive(Block)]
pub struct Split<
    F,
    A,
    B,
    C,
    I = circular::Reader<A>,
    O1 = circular::Writer<B>,
    O2 = circular::Writer<C>,
> where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O1: CpuBufferWriter<Item = B>,
    O2: CpuBufferWriter<Item = C>,
{
    #[input]
    input: I,
    #[output]
    output1: O1,
    #[output]
    output2: O2,
    f: F,
}

impl<F, A, B, C, I, O1, O2> Split<F, A, B, C, I, O1, O2>
where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O1: CpuBufferWriter<Item = B>,
    O2: CpuBufferWriter<Item = C>,
{
    /// Create Split block
    pub fn new(f: F) -> Self {
        Self {
            input: I::default(),
            output1: O1::default(),
            output2: O2::default(),
            f,
        }
    }
}

#[doc(hidden)]
impl<F, A, B, C, I, O1, O2> Kernel for Split<F, A, B, C, I, O1, O2>
where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O1: CpuBufferWriter<Item = B>,
    O2: CpuBufferWriter<Item = C>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i0 = self.input.slice();
        let o0 = self.output1.slice();
        let o1 = self.output2.slice();
        let i0_len = i0.len();

        let m = std::cmp::min(i0.len(), o0.len());
        let m = std::cmp::min(m, o1.len());

        if m > 0 {
            for (x, (y0, y1)) in i0.iter().zip(o0.iter_mut().zip(o1.iter_mut())) {
                let (a, b) = (self.f)(x);
                *y0 = a;
                *y1 = b;
            }

            self.input.consume(m);
            self.output1.produce(m);
            self.output2.produce(m);
        }

        if self.input.finished() && m == i0_len {
            io.finished = true;
        }

        Ok(())
    }
}
