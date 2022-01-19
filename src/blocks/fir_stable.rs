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

pub trait HasFirImpl: Copy + Send + 'static {}
impl HasFirImpl for f32 {}

pub struct Fir<A>
where
    A: HasFirImpl,
{
    taps: Box<[A]>,
}

impl<A> Fir<A>
where
    A: HasFirImpl,
    Fir<A>: SyncKernel,
{
    pub fn new(taps: &[A]) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<A>())
                .build(),
            MessageIoBuilder::<Fir<A>>::new().build(),
            Fir {
                taps: taps.to_vec().into_boxed_slice(),
            },
        )
    }
}

#[async_trait]
impl SyncKernel for Fir<f32> {
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n_taps = self.taps.len();

        if i.len() >= n_taps {
            let n = std::cmp::min(i.len() + 1 - n_taps, o.len());

            unsafe {
                for k in 0..n {
                    let mut sum = 0.0;
                    for t in 0..n_taps {
                        sum += i.get_unchecked(k + t) * self.taps.get_unchecked(t);
                    }
                    *o.get_unchecked_mut(k) = sum;
                }
            }

            sio.input(0).consume(n);
            sio.output(0).produce(n);

            if sio.input(0).finished() && n == i.len() + 1 - n_taps {
                io.finished = true;
            }
        } else if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
