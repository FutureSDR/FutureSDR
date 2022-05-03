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
use futuredsp::iir::IirKernel;
use futuredsp::{StatefulUnaryKernel, TapsAccessor};

pub struct Iir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + StatefulUnaryKernel<SampleType>,
{
    core: Core,
    _sampletype: std::marker::PhantomData<SampleType>,
    _taptype: std::marker::PhantomData<TapType>,
}

unsafe impl<SampleType, TapType, Core> Send for Iir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + StatefulUnaryKernel<SampleType>,
{
}

impl<SampleType, TapType, Core> Iir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + StatefulUnaryKernel<SampleType>,
{
    pub fn new(core: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Iir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<SampleType>())
                .add_output("out", mem::size_of::<SampleType>())
                .build(),
            MessageIoBuilder::<Iir<SampleType, TapType, Core>>::new().build(),
            Iir {
                core,
                _sampletype: std::marker::PhantomData,
                _taptype: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<SampleType, TapType, Core> Kernel for Iir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + StatefulUnaryKernel<SampleType>,
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

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && status.produced_all_samples() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Creates a generic IIR filter.
///
/// This filter consumes two sets of taps, `a_taps` and `b_taps`. `a_taps` are
/// the feedback taps, and `b_taps` are the feed-forward taps. If there are `n`
/// feed-forward taps and `m` feed-back taps, the equation is:
/// ```text
/// y(k) = x[k] * b[0] + x[k-1] * b[1] + ... + x[k-n] * b[n]
///        + x[k-1] * a[0] + x[k-2] * a[1] + ... + x[k-m-1] * a[m]
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
/// use futuresdr::blocks::IirBuilder;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let iir = fg.add_block(IirBuilder::new::<f32, f32, _>([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]));
/// ```
pub struct IirBuilder {
    //
}

impl IirBuilder {
    pub fn new<SampleType, TapType, Taps>(a_taps: Taps, b_taps: Taps) -> Block
    where
        SampleType: 'static + Send + Clone,
        TapType: 'static,
        Taps: 'static + TapsAccessor,
        IirKernel<SampleType, Taps>: StatefulUnaryKernel<SampleType>,
    {
        Iir::<SampleType, TapType, IirKernel<SampleType, Taps>>::new(IirKernel::new(a_taps, b_taps))
    }
}
