use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Repeatedly apply a function to generate samples, using [Option] values to allow termination.
#[derive(Block)]
pub struct FiniteSource<F, A>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
{
    f: F,
    _p: std::marker::PhantomData<A>,
}

impl<F, A> FiniteSource<F, A>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
{
    /// Create FiniteSource block
    pub fn new(f: F) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_output::<A>("out").build(),
            Self {
                f,
                _p: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
impl<F, A> Kernel for FiniteSource<F, A>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<A>();

        for (i, v) in o.iter_mut().enumerate() {
            if let Some(x) = (self.f)() {
                *v = x;
            } else {
                sio.output(0).produce(i);
                io.finished = true;
                return Ok(());
            }
        }

        sio.output(0).produce(o.len());

        Ok(())
    }
}
