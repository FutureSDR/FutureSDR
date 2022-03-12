use std::mem;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;
use futuredsp::fir::*;
use futuredsp::{TapsAccessor, UnaryKernel};

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
        Block::new_sync(
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
impl<SampleType, TapType, Core> SyncKernel for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + UnaryKernel<SampleType>,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<SampleType>();
        let o = sio.output(0).slice::<SampleType>();

        let (consumed, produced, status) = self.core.work(i, o);

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
/// ```
pub struct FirBuilder {
    //
}

impl FirBuilder {
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
}
