#![allow(clippy::type_complexity)]
use crate::prelude::*;
use futuredsp::ComputationStatus;
use futuredsp::IirFilter;
use futuredsp::prelude::*;

/// IIR filter.
#[derive(Block)]
pub struct Iir<
    InputType,
    OutputType,
    TapsType,
    Core,
    I = DefaultCpuReader<InputType>,
    O = DefaultCpuWriter<OutputType>,
> where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapsType: 'static + Send + Taps,
    Core: 'static + StatefulFilter<InputType, OutputType, TapsType::TapType> + Send,
    I: CpuBufferReader<Item = InputType>,
    O: CpuBufferWriter<Item = OutputType>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    core: Core,
    _tap_type: std::marker::PhantomData<TapsType>,
}

impl<InputType, OutputType, TapsType, I, O>
    Iir<InputType, OutputType, TapsType, IirFilter<InputType, OutputType, TapsType>, I, O>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapsType: 'static + Send + Taps,
    IirFilter<InputType, OutputType, TapsType>:
        StatefulFilter<InputType, OutputType, TapsType::TapType>,
    I: CpuBufferReader<Item = InputType>,
    O: CpuBufferWriter<Item = OutputType>,
{
    /// Build an [Iir] filter.
    ///
    /// This filter consumes two sets of taps, `a_taps` and `b_taps`. `a_taps` are
    /// the feedback taps, and `b_taps` are the feed-forward taps. If there are `n`
    /// feed-forward taps and `m` feed-back taps, the equation is:
    /// ```text
    /// y(k) = x[k] * b[0] + x[k-1] * b[1] + ... + x[k-n] * b[n]
    ///        + y[k-1] * a[0] + y[k-2] * a[1] + ... + y[k-m-1] * a[m]
    /// ```
    ///
    /// Uses the `futuredsp` to pick the optimal IIR implementation for the given
    /// constraints.
    ///
    /// Note that there must be an implementation of [futuredsp::TapsAccessor] for
    /// the taps objects you pass in, see docs for details. Both the a_taps and the
    /// b_taps objects must be the same type.
    ///
    /// Additionally, there must be an available core (implementation of
    /// [futuredsp::StatefulUnaryKernel]) available for the specified `SampleType`
    /// and `TapsType`. See the [futuredsp docs](futuredsp::iir) for available
    /// implementations.
    ///
    /// # Inputs
    ///
    /// `in`: Input
    ///
    /// # Outputs
    ///
    /// `out`: Output
    ///
    /// # Usage
    /// ```
    /// use futuresdr::blocks::Iir;
    ///
    /// let iir: Iir<_, _, _, _> = Iir::new([1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]);
    /// ```
    pub fn new(
        a_taps: TapsType,
        b_taps: TapsType,
    ) -> Iir<InputType, OutputType, TapsType, IirFilter<InputType, OutputType, TapsType>, I, O>
    {
        Iir::with_core(IirFilter::new(a_taps, b_taps))
    }
}

impl<InputType, OutputType, TapsType, Core, I, O> Iir<InputType, OutputType, TapsType, Core, I, O>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapsType: 'static + Send + Taps,
    Core: 'static + StatefulFilter<InputType, OutputType, TapsType::TapType> + Send,
    IirFilter<InputType, OutputType, TapsType>:
        StatefulFilter<InputType, OutputType, TapsType::TapType>,
    I: CpuBufferReader<Item = InputType>,
    O: CpuBufferWriter<Item = OutputType>,
{
    /// Create IIR filter block
    pub fn with_core(core: Core) -> Self {
        let mut input = I::default();
        input.set_min_items(core.length());
        Self {
            input,
            output: O::default(),
            core,
            _tap_type: std::marker::PhantomData,
        }
    }
}

#[doc(hidden)]
impl<InputType, OutputType, TapsType, Core, I, O> Kernel
    for Iir<InputType, OutputType, TapsType, Core, I, O>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapsType: 'static + Send + Taps,
    Core: 'static + StatefulFilter<InputType, OutputType, TapsType::TapType> + Send,
    IirFilter<InputType, OutputType, TapsType>:
        StatefulFilter<InputType, OutputType, TapsType::TapType>,
    I: CpuBufferReader<Item = InputType>,
    O: CpuBufferWriter<Item = OutputType>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();

        let (consumed, produced, status) = self.core.filter(i, o);

        self.input.consume(consumed);
        self.output.produce(produced);

        if self.input.finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}
