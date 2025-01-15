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

/// Apply a function to received samples.
///
/// # Inputs
///
/// `in` Input Samples.
///
/// # Outputs
///
/// No Outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::Sink;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(Sink::new(|x: &f32| println!("{}", x)));
/// ```
#[derive(Block)]
pub struct Sink<F, A>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
{
    f: F,
    _p: std::marker::PhantomData<A>,
}

impl<F, A> Sink<F, A>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
{
    /// Create Sink block
    pub fn new(f: F) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Sink").build(),
            StreamIoBuilder::new().add_input::<A>("in").build(),
            MessageOutputsBuilder::new().build(),
            Self {
                f,
                _p: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
impl<F, A> Kernel for Sink<F, A>
where
    F: FnMut(&A) + Send + 'static,
    A: Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();

        for v in i.iter() {
            (self.f)(v);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(i.len());

        Ok(())
    }
}
