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

/// Repeatedly apply a function to generate samples.
///
/// # Inputs
///
/// No inputs.
///
/// # Outputs
///
/// `out`: Output samples
///
/// # Usage
/// ```
/// use futuresdr::blocks::Source;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// // Generate zeroes
/// let source = fg.add_block(Source::new(|| { 0.0f32 }));
/// ```
#[derive(Block)]
pub struct Source<F, A>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
{
    f: F,
    _p: std::marker::PhantomData<A>,
}

impl<F, A> Source<F, A>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
{
    /// Create Source block
    pub fn new(f: F) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Source").build(),
            StreamIoBuilder::new().add_output::<A>("out").build(),
            MessageOutputsBuilder::new().build(),
            Self {
                f,
                _p: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
impl<F, A> Kernel for Source<F, A>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<A>();

        for v in o.iter_mut() {
            *v = (self.f)();
        }

        sio.output(0).produce(o.len());

        Ok(())
    }
}
