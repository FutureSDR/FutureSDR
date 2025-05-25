#![allow(clippy::type_complexity)]
use futuredsp::firdes;
use futuredsp::prelude::*;
use futuredsp::ComputationStatus;
use futuredsp::DecimatingFirFilter;
use futuredsp::FirFilter;
use futuredsp::MmseResampler;
use futuredsp::PolyphaseResamplingFir;
use num_traits::Num;
use std::iter::Sum;
use std::ops::Mul;

use crate::prelude::*;

/// FIR filter.
#[derive(Block)]
pub struct Fir<
    InputType,
    OutputType,
    TapType,
    Core,
    IN = DefaultCpuReader<InputType>,
    OUT = DefaultCpuWriter<OutputType>,
> where
    InputType: CpuSample,
    OutputType: CpuSample,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType> + Send,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    #[input]
    input: IN,
    #[output]
    output: OUT,
    filter: Core,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Core, IN, OUT>
    Fir<InputType, OutputType, TapType, Core, IN, OUT>
where
    InputType: CpuSample,
    OutputType: CpuSample,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType> + Send + 'static,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    /// Create FIR block
    pub fn new(filter: Core) -> Self {
        let mut input = IN::default();
        input.set_min_items(filter.length());
        Self {
            input,
            output: OUT::default(),
            filter,
            _tap_type: std::marker::PhantomData,
        }
    }
}

#[doc(hidden)]
impl<InputType, OutputType, TapType, Core, IN, OUT> Kernel
    for Fir<InputType, OutputType, TapType, Core, IN, OUT>
where
    InputType: CpuSample,
    OutputType: CpuSample,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType> + Send + 'static,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();

        let (consumed, produced, status) = self.filter.filter(i, o);

        self.input.consume(consumed);
        self.output.produce(produced);

        if self.input.finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}

/// Stateful FIR filter.
#[derive(Block)]
pub struct StatefulFir<
    InputType,
    OutputType,
    TapType,
    Core,
    IN = DefaultCpuReader<InputType>,
    OUT = DefaultCpuWriter<OutputType>,
> where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType> + Send,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    #[input]
    input: IN,
    #[output]
    output: OUT,
    filter: Core,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Core, IN, OUT>
    StatefulFir<InputType, OutputType, TapType, Core, IN, OUT>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType> + Send + 'static,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    /// Create FIR block
    pub fn new(filter: Core) -> Self {
        Self {
            input: IN::default(),
            output: OUT::default(),
            filter,
            _tap_type: std::marker::PhantomData,
        }
    }
}

#[doc(hidden)]
impl<InputType, OutputType, TapType, Core, IN, OUT> Kernel
    for StatefulFir<InputType, OutputType, TapType, Core, IN, OUT>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType> + Send + 'static,
    IN: CpuBufferReader<Item = InputType>,
    OUT: CpuBufferWriter<Item = OutputType>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();

        let (consumed, produced, status) = self.filter.filter(i, o);

        self.input.consume(consumed);
        self.output.produce(produced);

        if self.input.finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}

/// Create a [Fir] filter.
///
/// Uses `futuredsp` to pick the optimal FIR implementation for the given
/// constraints.
///
/// Note that there must be an implementation of [futuredsp::TapsAccessor] for
/// the taps object you pass in, see docs for details.
///
/// Additionally, there must be an available core (implementation of
/// [futuredsp::UnaryKernel]) available for the specified `SampleType` and
/// `TapsType`. See the [futuredsp docs](futuredsp::fir) for available
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
/// use futuresdr::blocks::FirBuilder;
/// use num_complex::Complex;
///
/// let fir = FirBuilder::fir::<f32, f32, _>([1.0f32, 2.0, 3.0]);
/// let fir = FirBuilder::fir::<Complex<f32>, Complex<f32>, _>(&[1.0f32, 2.0, 3.0]);
/// let fir = FirBuilder::fir::<f32, f32, _>(vec![1.0f32, 2.0, 3.0]);
/// let fir = FirBuilder::resampling_with_taps::<f32, f32, _>(3, 2, vec![1.0f32, 2.0, 3.0]);
/// let fir = FirBuilder::mmse::<f32>(2.0);
/// ```
pub struct FirBuilder;

impl FirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn fir<InputType, OutputType, TapsType>(
        taps: TapsType,
    ) -> Fir<InputType, OutputType, TapsType::TapType, FirFilter<InputType, OutputType, TapsType>>
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        TapsType: 'static + Taps + Send,
        TapsType::TapType: 'static + Send,
        FirFilter<InputType, OutputType, TapsType>:
            futuredsp::Filter<InputType, OutputType, TapsType::TapType>,
    {
        Fir::<InputType, OutputType, TapsType::TapType, FirFilter<InputType, OutputType, TapsType>>::new(FirFilter::new(taps))
    }

    /// Create a decimating FIR filter with standard low-pass taps.
    pub fn decimating<InputType, OutputType, TapsType>(
        decim: usize,
    ) -> Fir<InputType, OutputType, f32, DecimatingFirFilter<InputType, OutputType, Vec<f32>>>
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        DecimatingFirFilter<InputType, OutputType, Vec<f32>>:
            futuredsp::Filter<InputType, OutputType, f32>,
    {
        let taps = firdes::kaiser::lowpass::<f32>(1.0 / decim as f64, 0.1, 0.0001);
        FirBuilder::decimating_with_taps(decim, taps)
    }

    /// Create a decimating FIR filter with the specified taps.
    pub fn decimating_with_taps<InputType, OutputType, TapsType>(
        decim: usize,
        taps: TapsType,
    ) -> Fir<
        InputType,
        OutputType,
        TapsType::TapType,
        DecimatingFirFilter<InputType, OutputType, TapsType>,
    >
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        TapsType: 'static + Taps + Send,
        TapsType::TapType: 'static + Send,
        DecimatingFirFilter<InputType, OutputType, TapsType>:
            futuredsp::Filter<InputType, OutputType, TapsType::TapType>,
    {
        Fir::<
            InputType,
            OutputType,
            TapsType::TapType,
            DecimatingFirFilter<InputType, OutputType, TapsType>,
        >::new(DecimatingFirFilter::new(decim, taps))
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim`. The interpolation filter is constructed
    /// using default parameters.
    pub fn resampling<InputType, OutputType>(
        interp: usize,
        decim: usize,
    ) -> Fir<InputType, OutputType, f32, PolyphaseResamplingFir<InputType, OutputType, Vec<f32>>>
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        PolyphaseResamplingFir<InputType, OutputType, Vec<f32>>: Filter<InputType, OutputType, f32>,
    {
        // Reduce factors
        let gcd = num_integer::gcd(interp, decim);
        let interp = interp / gcd;
        let decim = decim / gcd;
        // Design filter
        let taps = firdes::kaiser::multirate::<f32>(interp, decim, 12, 0.0001);
        FirBuilder::resampling_with_taps::<InputType, OutputType, _>(interp, decim, taps)
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim` and uses `taps` as the interpolation/decimation filter.
    /// The length of `taps` must be divisible by `interp`.
    pub fn resampling_with_taps<InputType, OutputType, TapsType>(
        interp: usize,
        decim: usize,
        taps: TapsType,
    ) -> Fir<
        InputType,
        OutputType,
        TapsType::TapType,
        PolyphaseResamplingFir<InputType, OutputType, TapsType>,
    >
    where
        InputType: CpuSample,
        OutputType: CpuSample,
        TapsType: 'static + Taps + Send,
        TapsType::TapType: 'static + Send,
        PolyphaseResamplingFir<InputType, OutputType, TapsType>:
            Filter<InputType, OutputType, TapsType::TapType>,
    {
        Fir::<
            InputType,
            OutputType,
            TapsType::TapType,
            PolyphaseResamplingFir<InputType, OutputType, TapsType>,
        >::new(PolyphaseResamplingFir::new(interp, decim, taps))
    }
    /// Create a new MMSE Resampler.
    pub fn mmse<SampleType>(
        ratio: f32,
    ) -> StatefulFir<SampleType, SampleType, f32, MmseResampler<SampleType>>
    where
        SampleType:
            CpuSample + Copy + Num + Sum<SampleType> + Mul<f32, Output = SampleType> + 'static,
        MmseResampler<SampleType>: StatefulFilter<SampleType, SampleType, f32>,
    {
        StatefulFir::<SampleType, SampleType, f32, MmseResampler<SampleType>>::new(
            MmseResampler::new(ratio),
        )
    }
}
