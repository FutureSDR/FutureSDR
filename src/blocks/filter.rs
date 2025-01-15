use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Apply a function, returning an [Option] to allow filtering samples.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Filtered outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::Filter;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// // Remove samples above 1.0
/// let filter = fg.add_block(Filter::<f32, f32>::new(|i| {
///     if *i < 1.0 {
///         Some(*i)
///     } else {
///         None
///     }
/// }));
/// ```
#[allow(clippy::type_complexity)]
#[derive(Block)]
pub struct Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    f: Box<dyn FnMut(&A) -> Option<B> + Send + 'static>,
}

impl<A, B> Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    /// Create Filter block
    pub fn new(f: impl FnMut(&A) -> Option<B> + Send + 'static) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Filter").build(),
            StreamIoBuilder::new()
                .add_input::<A>("in")
                .add_output::<B>("out")
                .build(),
            MessageOutputsBuilder::new().build(),
            Self { f: Box::new(f) },
        )
    }
}

#[doc(hidden)]
impl<A, B> Kernel for Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();
        let o = sio.output(0).slice::<B>();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            if consumed >= i.len() {
                break;
            }
            if let Some(v) = (self.f)(&i[consumed]) {
                o[produced] = v;
                produced += 1;
            }
            consumed += 1;
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && consumed == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
