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

/// FIR filter.
pub struct Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType>,
{
    filter: Core,
    _input_type: std::marker::PhantomData<InputType>,
    _output_type: std::marker::PhantomData<OutputType>,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Core> Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType> + Send + 'static,
{
    /// Create FIR block
    pub fn new(filter: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input::<InputType>("in")
                .add_output::<OutputType>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Self {
                filter,
                _input_type: std::marker::PhantomData,
                _output_type: std::marker::PhantomData,
                _tap_type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<InputType, OutputType, TapType, Core> Kernel for Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: Filter<InputType, OutputType, TapType> + Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<InputType>();
        let o = sio.output(0).slice::<OutputType>();

        let (consumed, produced, status) = self.filter.filter(i, o);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}

/// Stateful FIR filter.
pub struct StatefulFir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType>,
{
    filter: Core,
    _input_type: std::marker::PhantomData<InputType>,
    _output_type: std::marker::PhantomData<OutputType>,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Core> StatefulFir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType> + Send + 'static,
{
    /// Create FIR block
    pub fn new(filter: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input::<InputType>("in")
                .add_output::<OutputType>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Self {
                filter,
                _input_type: std::marker::PhantomData,
                _output_type: std::marker::PhantomData,
                _tap_type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<InputType, OutputType, TapType, Core> Kernel
    for StatefulFir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: StatefulFilter<InputType, OutputType, TapType> + Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<InputType>();
        let o = sio.output(0).slice::<OutputType>();

        let (consumed, produced, status) = self.filter.filter(i, o);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
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
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _>([1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<Complex<f32>, Complex<f32>, _>(&[1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _>(vec![1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::resampling_with_taps::<f32, f32, _>(3, 2, vec![1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::mmse::<f32>(2.0));
/// ```
pub struct FirBuilder;

impl FirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn new<InputType, OutputType, TapsType>(taps: TapsType) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        TapsType: 'static + Taps + Send,
        TapsType::TapType: 'static + Send,
        FirFilter<InputType, OutputType, TapsType>:
            futuredsp::Filter<InputType, OutputType, TapsType::TapType>,
    {
        Fir::<InputType, OutputType, TapsType::TapType, FirFilter<InputType, OutputType, TapsType>>::new(FirFilter::new(taps))
    }

    /// Create a decimating FIR filter with standard low-pass taps.
    pub fn decimating<InputType, OutputType, TapsType>(decim: usize) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
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
    ) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
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
    pub fn resampling<InputType, OutputType>(interp: usize, decim: usize) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
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
    ) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
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
    pub fn mmse<SampleType>(ratio: f32) -> Block
    where
        SampleType: Copy + Send + Num + Sum<SampleType> + Mul<f32, Output = SampleType> + 'static,
        MmseResampler<SampleType>: StatefulFilter<SampleType, SampleType, f32>,
    {
        StatefulFir::<SampleType, SampleType, f32, MmseResampler<SampleType>>::new(
            MmseResampler::new(ratio),
        )
    }
}
