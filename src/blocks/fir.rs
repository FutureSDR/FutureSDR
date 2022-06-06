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

pub struct Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<SampleType>,
{
    core: Core,
    _sampletype: std::marker::PhantomData<SampleType>,
    _taptype: std::marker::PhantomData<TapType>,
}

unsafe impl<SampleType, TapType, Core> Send for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<SampleType>,
{
}

impl<SampleType, TapType, Core> Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<SampleType>,
{
    pub fn new(core: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<SampleType>())
                .add_output("out", mem::size_of::<SampleType>())
                .build(),
            MessageIoBuilder::<Fir<SampleType, TapType, Core>>::new().build(),
            Fir {
                core,
                _sampletype: std::marker::PhantomData,
                _taptype: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<SampleType, TapType, Core> Kernel for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<SampleType>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<SampleType>();
        let o = sio.output(0).slice::<SampleType>();

        let (consumed, produced, status) = self.core.work(i, o);

        println!("i.len {}    o.len {}   consumed {}   produced {}   status {:?}", i.len(), o.len(), consumed, produced, status);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && status.produced_all_samples() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Creates a generic FIR filter.
///
/// Uses the `futuredsp` to pick the optimal FIR implementation for the given
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
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _>([1.0, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<Complex<f32>, f32, _>(&[1.0, 2.0, 3.0]));
/// let fir = fg.add_block(FirBuilder::new::<f32, f32, _>(vec![1.0, 2.0, 3.0]));
///
/// let fir = fg.add_block(FirBuilder::new_resampling_with_taps::<f32, f32, _>(3, 2, vec![1.0, 2.0, 3.0]));
/// ```
pub struct FirBuilder {
    //
}

impl FirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn new<SampleType, TapType, Taps>(taps: Taps) -> Block
    where
        SampleType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor,
        NonResamplingFirKernel<SampleType, Taps>: UnaryKernel<SampleType>,
    {
        Fir::<SampleType, TapType, NonResamplingFirKernel<SampleType, Taps>>::new(
            NonResamplingFirKernel::new(taps),
        )
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim`. The interpolation filter is constructed
    /// using default parameters.
    pub fn new_resampling<SampleType>(interp: usize, decim: usize) -> Block
    where
        SampleType: 'static + Send,
        PolyphaseResamplingFirKernel<SampleType, Vec<f32>>: UnaryKernel<SampleType>,
    {
        // Reduce factors
        let gcd = num_integer::gcd(interp, decim);
        let interp = interp / gcd;
        let decim = decim / gcd;
        // Design filter
        let taps = firdes::kaiser::multirate::<f32>(interp, decim, 12, 0.0001);
        FirBuilder::new_resampling_with_taps::<SampleType, f32, _>(interp, decim, taps)
    }

    /// Create a new rationally resampling FIR filter that changes the sampling
    /// rate by a factor `interp/decim` and uses `taps` as the interpolation/decimation filter.
    /// The length of `taps` must be divisible by `interp`.
    pub fn new_resampling_with_taps<SampleType, TapType, Taps>(
        interp: usize,
        decim: usize,
        taps: Taps,
    ) -> Block
    where
        SampleType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor,
        PolyphaseResamplingFirKernel<SampleType, Taps>: UnaryKernel<SampleType>,
    {
        let gcd = num_integer::gcd(interp, decim);
        let interp = interp / gcd;
        let decim = decim / gcd;
        Fir::<SampleType, TapType, PolyphaseResamplingFirKernel<SampleType, Taps>>::new(
            PolyphaseResamplingFirKernel::new(interp, decim, taps),
        )
    }
}
