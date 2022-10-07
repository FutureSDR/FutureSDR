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

/// Apply a function to split a stream.
pub struct Split<F, A, B, C>
where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    f: F,
    _p1: std::marker::PhantomData<A>,
    _p2: std::marker::PhantomData<B>,
    _p3: std::marker::PhantomData<C>,
}

impl<F, A, B, C> Split<F, A, B, C>
where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    pub fn new(f: F) -> Block {
        Block::new(
            BlockMetaBuilder::new("Split").build(),
            StreamIoBuilder::new()
                .add_input::<A>("in")
                .add_output::<B>("out0")
                .add_output::<C>("out1")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Split {
                f,
                _p1: std::marker::PhantomData,
                _p2: std::marker::PhantomData,
                _p3: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A, B, C> Kernel for Split<F, A, B, C>
where
    F: FnMut(&A) -> (B, C) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i0 = sio.input(0).slice::<A>();
        let o0 = sio.output(0).slice::<B>();
        let o1 = sio.output(1).slice::<C>();

        let m = std::cmp::min(i0.len(), o0.len());
        let m = std::cmp::min(m, o1.len());

        if m > 0 {
            for (x, (y0, y1)) in i0.iter().zip(o0.iter_mut().zip(o1.iter_mut())) {
                let (a, b) = (self.f)(x);
                *y0 = a;
                *y1 = b;
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
            sio.output(1).produce(m);
        }

        if sio.input(0).finished() && m == i0.len() {
            io.finished = true;
        }

        Ok(())
    }
}
