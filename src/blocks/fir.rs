use std::mem;

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
use futuredsp::fir::*;
use futuredsp::firdes;
use futuredsp::{TapsAccessor, UnaryKernel};
use num_integer;

/// FIR filter.
pub struct Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<InputType, OutputType>,
{
    core: Core,
    _input_type: std::marker::PhantomData<InputType>,
    _output_type: std::marker::PhantomData<OutputType>,
    _tap_type: std::marker::PhantomData<TapType>,
}

unsafe impl<InputType, OutputType, TapType, Core> Send for Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<InputType, OutputType>,
{
}

impl<InputType, OutputType, TapType, Core> Fir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<InputType, OutputType>,
{
    pub fn new(core: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<InputType>())
                .add_output("out", mem::size_of::<OutputType>())
                .build(),
            MessageIoBuilder::<Fir<InputType, OutputType, TapType, Core>>::new().build(),
            Fir {
                core,
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
    TapType: 'static,
    Core: 'static + UnaryKernel<InputType, OutputType>,
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

        let (consumed, produced, status) = self.core.work(i, o);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && status.produced_all_samples() {
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
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, f32, _>([1.0, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<Complex<f32>, Complex<f32>, f32, _>(&[1.0, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, f32, _>(vec![1.0, 2.0, 3.0]));
///
/// let fir = fg.add_block(FirBuilder::new_resampling_with_taps::<f32, f32, f32, _>(3, 2, vec![1.0, 2.0, 3.0]));
/// ```
pub struct FirBuilder {
    //
}

impl FirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn new<InputType, OutputType, TapType, Taps>(taps: Taps) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor<TapType = TapType>,
        NonResamplingFirKernel<InputType, OutputType, Taps, TapType>:
            UnaryKernel<InputType, OutputType>,
    {
        Fir::<
            InputType,
            OutputType,
            TapType,
            NonResamplingFirKernel<InputType, OutputType, Taps, TapType>,
        >::new(NonResamplingFirKernel::new(taps))
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim`. The interpolation filter is constructed
    /// using default parameters.
    pub fn new_resampling<InputType, OutputType>(interp: usize, decim: usize) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        PolyphaseResamplingFirKernel<InputType, OutputType, Vec<f32>, f32>:
            UnaryKernel<InputType, OutputType>,
    {
        // Reduce factors
        let gcd = num_integer::gcd(interp, decim);
        let interp = interp / gcd;
        let decim = decim / gcd;
        // Design filter
        let taps = firdes::kaiser::multirate::<f32>(interp, decim, 12, 0.0001);
        FirBuilder::new_resampling_with_taps::<InputType, OutputType, f32, _>(interp, decim, taps)
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim` and uses `taps` as the interpolation/decimation filter.
    /// The length of `taps` must be divisible by `interp`.
    pub fn new_resampling_with_taps<InputType, OutputType, TapType, Taps>(
        interp: usize,
        decim: usize,
        taps: Taps,
    ) -> Block
    where
        InputType: 'static + Send,
        OutputType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor<TapType = TapType>,
        PolyphaseResamplingFirKernel<InputType, OutputType, Taps, TapType>:
            UnaryKernel<InputType, OutputType>,
    {
        Fir::<
            InputType,
            OutputType,
            TapType,
            PolyphaseResamplingFirKernel<InputType, OutputType, Taps, TapType>,
        >::new(PolyphaseResamplingFirKernel::new(interp, decim, taps))
    }
}
