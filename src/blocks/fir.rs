use futuredsp::firdes;
use futuredsp::ComputationStatus;
use futuredsp::FirFilter;
use futuredsp::FirKernel;
use futuredsp::PolyphaseResamplingFir;
use futuredsp::Taps;

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
pub struct Fir<InputType, OutputType, TapType, Filter>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Filter: 'static + FirKernel<InputType, OutputType> + Send,
{
    filter: Filter,
    _input_type: std::marker::PhantomData<InputType>,
    _output_type: std::marker::PhantomData<OutputType>,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Filter> Fir<InputType, OutputType, TapType, Filter>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Filter: 'static + FirKernel<InputType, OutputType> + Send,
{
    /// Create FIR block
    pub fn new(filter: Filter) -> Block {
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
impl<InputType, OutputType, TapType, Filter> Kernel for Fir<InputType, OutputType, TapType, Filter>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Filter: 'static + FirKernel<InputType, OutputType> + Send,
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
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _, _>([1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<Complex<f32>, Complex<f32>, _, _>(&[1.0f32, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _, _>(vec![1.0f32, 2.0, 3.0]));
///
/// let fir = fg.add_block(FirBuilder::new_resampling_with_taps::<f32, f32, f32, _>(3, 2, vec![1.0f32, 2.0, 3.0]));
/// ```
pub struct FirBuilder;

impl FirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn new<InputType, OutputType, TapsType, TapType>(taps: TapsType) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        TapsType: 'static + Taps<TapType = TapType> + Send,
        TapType: 'static + Send,
        FirFilter<InputType, OutputType, TapsType, TapType>: FirKernel<InputType, OutputType>,
    {
        Fir::<InputType, OutputType, TapType, FirFilter<InputType, OutputType, TapsType, TapType>>::new(FirFilter::new(taps))
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim`. The interpolation filter is constructed
    /// using default parameters.
    pub fn new_resampling<InputType, OutputType>(interp: usize, decim: usize) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        PolyphaseResamplingFir<InputType, OutputType, Vec<f32>, f32>:
            FirKernel<InputType, OutputType>,
    {
        // Reduce factors
        let gcd = num_integer::gcd(interp, decim);
        let interp = interp / gcd;
        let decim = decim / gcd;
        // Design filter
        let taps = firdes::kaiser::multirate::<f32>(interp, decim, 12, 0.0001);
        FirBuilder::new_resampling_with_taps::<InputType, OutputType, _, f32>(interp, decim, taps)
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim` and uses `taps` as the interpolation/decimation filter.
    /// The length of `taps` must be divisible by `interp`.
    pub fn new_resampling_with_taps<InputType, OutputType, TapsType, TapType>(
        interp: usize,
        decim: usize,
        taps: TapsType,
    ) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        TapType: 'static + Send,
        TapsType: 'static + Taps<TapType = TapType> + Send,
        PolyphaseResamplingFir<InputType, OutputType, TapsType, TapType>:
            FirKernel<InputType, OutputType>,
    {
        Fir::<
            InputType,
            OutputType,
            TapType,
            PolyphaseResamplingFir<InputType, OutputType, TapsType, TapType>,
        >::new(PolyphaseResamplingFir::new(interp, decim, taps))
    }
}
