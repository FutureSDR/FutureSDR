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

pub struct Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
    core: Core,
    _sampletype: std::marker::PhantomData<SampleType>,
    _taptype: std::marker::PhantomData<TapType>,
}

unsafe impl<SampleType, TapType, Core> Send for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
}

impl<SampleType, TapType, Core> Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
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
    Core: 'static + FirKernel<SampleType>,
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

        let (consumed, produced, additional_production) = self.core.work(i, o);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && additional_production == 0 {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct FirBuilder {
    //
}

impl FirBuilder {
    /// Constructs a new FIR Filter using the given types and taps. This function will
    /// pick the optimal FIR implementation for the given constraints.
    ///
    /// Note that there must be an implementation of `TapsAccessor` for the taps object
    /// you pass in. Implementations are provided for arrays and `Vec<TapType>`.
    ///
    /// Additionally, there must be an available core (implementation of `FirKernel`) for
    /// the specified `SampleType` and `TapType`. Cores are provided for the following
    /// `SampleType`/`TapType` combinations:
    /// - `SampleType=f32`, `TapType=f32`
    /// - `SampleType=Complex<f32>`, `TapType=f32`
    ///
    /// Example usage:
    /// ```
    /// use futuresdr::blocks::FirBuilder;
    /// use num_complex::Complex;
    ///
    /// let fir = FirBuilder::new::<f32, f32, _>([1.0, 2.0, 3.0]);
    /// let fir = FirBuilder::new::<Complex<f32>, f32, _>(&[1.0, 2.0, 3.0]);
    /// let fir = FirBuilder::new::<f32, f32, _>(vec![1.0, 2.0, 3.0]);
    /// ```
    pub fn new<SampleType, TapType, Taps>(taps: Taps) -> Block
    where
        SampleType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor,
        NonResamplingFirKernel<SampleType, Taps>: FirKernel<SampleType>,
    {
        Fir::<SampleType, TapType, NonResamplingFirKernel<SampleType, Taps>>::new(
            NonResamplingFirKernel::new(taps),
        )
    }
}
