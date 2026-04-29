#![allow(clippy::type_complexity)]
use crate::runtime::dev::prelude::*;
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

/// Create an [`Iir`] filter with default stream buffers.
pub struct IirBuilder;

impl IirBuilder {
    /// Create an IIR filter for the specified input and output sample types.
    pub fn iir<InputType, OutputType, TapsType>(
        a_taps: TapsType,
        b_taps: TapsType,
    ) -> Iir<InputType, OutputType, TapsType, IirFilter<InputType, OutputType, TapsType>>
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        TapsType: 'static + Send + Taps,
        IirFilter<InputType, OutputType, TapsType>:
            StatefulFilter<InputType, OutputType, TapsType::TapType>,
    {
        Iir::with_core(IirFilter::new(a_taps, b_taps))
    }

    /// Create an IIR filter where the input and output sample types are the same.
    pub fn same_type<SampleType, TapsType>(
        a_taps: TapsType,
        b_taps: TapsType,
    ) -> Iir<SampleType, SampleType, TapsType, IirFilter<SampleType, SampleType, TapsType>>
    where
        SampleType: CpuSample,
        TapsType: 'static + Send + Taps,
        IirFilter<SampleType, SampleType, TapsType>:
            StatefulFilter<SampleType, SampleType, TapsType::TapType>,
    {
        Self::iir(a_taps, b_taps)
    }
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
    /// Note that the taps objects must implement [`futuredsp::Taps`]. Both
    /// `a_taps` and `b_taps` must be the same type.
    ///
    /// Additionally, there must be an available stateful IIR implementation in
    /// `futuredsp` for the specified `SampleType` and `TapsType`.
    ///
    /// # Stream Inputs
    ///
    /// `input`: Input samples.
    ///
    /// # Stream Outputs
    ///
    /// `output`: Filtered output samples.
    ///
    /// # Usage
    /// ```
    /// use futuresdr::blocks::IirBuilder;
    ///
    /// let iir = IirBuilder::same_type([1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]);
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
        _mo: &mut MessageOutputs,
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
