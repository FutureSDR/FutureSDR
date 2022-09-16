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
    pub fn new(f: F) -> Block {
        Block::new(
            BlockMetaBuilder::new("Source").build(),
            StreamIoBuilder::new()
                .add_output("out", mem::size_of::<A>())
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Source {
                f,
                _p: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A> Kernel for Source<F, A>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
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
