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

/// IIR filter.
pub struct Iir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: 'static + StatefulUnaryKernel<InputType, OutputType> + Send,
{
    core: Core,
    _input_type: std::marker::PhantomData<InputType>,
    _output_type: std::marker::PhantomData<OutputType>,
    _tap_type: std::marker::PhantomData<TapType>,
}

impl<InputType, OutputType, TapType, Core> Iir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: 'static + StatefulUnaryKernel<InputType, OutputType> + Send,
{
    pub fn new(core: Core) -> Block {
        Block::new(
            BlockMetaBuilder::new("Iir").build(),
            StreamIoBuilder::new()
                .add_input::<InputType>("in")
                .add_output::<OutputType>("out")
                .build(),
            MessageIoBuilder::<Iir<InputType, OutputType, TapType, Core>>::new().build(),
            Iir {
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
impl<InputType, OutputType, TapType, Core> Kernel for Iir<InputType, OutputType, TapType, Core>
where
    InputType: 'static + Send,
    OutputType: 'static + Send,
    TapType: 'static + Send,
    Core: 'static + StatefulUnaryKernel<InputType, OutputType> + Send,
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

/// Build an [Iir] filter.
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
/// let iir = fg.add_block(IirBuilder::new::<f32, f32, f32, [f32; 3]>([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]));
/// ```
pub struct IirBuilder {
    //
}

impl IirBuilder {
    pub fn new<InputType, OutputType, TapType, Taps>(a_taps: Taps, b_taps: Taps) -> Block
    where
        InputType: 'static + Send + Clone,
        OutputType: 'static + Send + Clone,
        TapType: 'static + Send,
        Taps: 'static + TapsAccessor + Send,
        IirKernel<InputType, OutputType, Taps>: StatefulUnaryKernel<InputType, OutputType> + Send,
    {
        Iir::<InputType, OutputType, TapType, IirKernel<InputType, OutputType, Taps>>::new(
            IirKernel::new(a_taps, b_taps),
        )
    }
}
