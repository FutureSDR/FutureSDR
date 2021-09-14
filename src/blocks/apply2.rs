use anyhow::Result;
use std::mem;

use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;

pub struct Apply2<A, B>
where
    A: 'static,
    B: 'static,
{
    f: Box<dyn FnMut(&A, &A) -> B + Send + 'static>,
}

impl<A, B> Apply2<A, B>
where
    A: 'static,
    B: 'static,
{
    pub fn new(f: impl FnMut(&A, &A) -> B + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Apply2").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", mem::size_of::<A>())
                .add_stream_output("out", mem::size_of::<B>())
                .build(),
            MessageIoBuilder::<Apply2<A, B>>::new().build(),
            Apply2 { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A, B> SyncKernel for Apply2<A, B>
where
    A: 'static,
    B: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();
        let o = sio.output(0).slice::<B>();

        if i.len() < 1 {
            return Ok(())
        }
        // Always keep one item left in input queue
        let m = std::cmp::min(i.len()-1 , o.len());
        if m > 0 {
            let mut input_iter = i.iter();
            let mut output_iter = o.iter_mut();
            let mut v_n_minus_1 = input_iter.next().unwrap();
            for _i in 1..m {
                let v_n = input_iter.next().unwrap();
                let r = output_iter.next().unwrap();
                *r = (self.f)(v_n_minus_1, v_n);
                v_n_minus_1 = v_n;
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() /*&& m == i.len()*/ {
            io.finished = true;
        }

        Ok(())
    }
}
