use crate::prelude::*;

/// Apply a function to combine two streams into one.
///
/// # Inputs
///
/// `in0`: Input A
///
/// `in1`: Input B
///
/// # Outputs
///
/// `out`: Combined output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Combine;
///
/// let adder = Combine::<_, _, _, _>::new(|a: &f32, b: &f32| {
///     a + b
/// });
/// ```
#[derive(Block)]
#[allow(clippy::type_complexity)]
pub struct Combine<
    F,
    A,
    B,
    C,
    INA = circular::Reader<A>,
    INB = circular::Reader<B>,
    OUT = circular::Writer<C>,
> where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    INA: CpuBufferReader<Item = A>,
    INB: CpuBufferReader<Item = B>,
    OUT: CpuBufferWriter<Item = C>,
{
    #[input]
    in0: INA,
    #[input]
    in1: INB,
    #[output]
    output: OUT,
    f: F,
}

impl<F, A, B, C, INA, INB, OUT> Combine<F, A, B, C, INA, INB, OUT>
where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    INA: CpuBufferReader<Item = A>,
    INB: CpuBufferReader<Item = B>,
    OUT: CpuBufferWriter<Item = C>,
{
    /// Create [`Combine`] block
    ///
    /// ## Parameter
    /// - `f`: Function `(&A, &B) -> C` used to combine samples
    pub fn new(f: F) -> Self {
        Self {
            in0: INA::default(),
            in1: INB::default(),
            output: OUT::default(),
            f,
        }
    }
}

#[doc(hidden)]
impl<F, A, B, C, INA, INB, OUT> Kernel for Combine<F, A, B, C, INA, INB, OUT>
where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
    INA: CpuBufferReader<Item = A>,
    INB: CpuBufferReader<Item = B>,
    OUT: CpuBufferWriter<Item = C>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i0 = self.in0.slice();
        let i1 = self.in1.slice();
        let o0 = self.output.slice();
        let i0_len = i0.len();
        let i1_len = i1.len();

        let m = std::cmp::min(i0.len(), i1.len());
        let m = std::cmp::min(m, o0.len());

        if m > 0 {
            for ((x0, x1), y) in i0.iter().zip(i1.iter()).zip(o0.iter_mut()) {
                *y = (self.f)(x0, x1);
            }

            self.in0.consume(m);
            self.in1.consume(m);
            self.output.produce(m);
        }

        if self.in0.finished() && m == i0_len {
            io.finished = true;
        }

        if self.in1.finished() && m == i1_len {
            io.finished = true;
        }

        Ok(())
    }
}
