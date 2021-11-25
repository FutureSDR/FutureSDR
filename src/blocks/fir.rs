use std::intrinsics::fadd_fast;
use std::intrinsics::fmul_fast;
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

pub trait HasFirImpl: Send + 'static {}
impl HasFirImpl for f32 {}

pub struct Fir<A, const N: usize>
where
    A: HasFirImpl,
{
    taps: [A; N],
}

impl<A, const N: usize> Fir<A, N>
where
    A: HasFirImpl,
    Fir<A, N>: SyncKernel,
{
    pub fn new(taps: [A; N]) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<A>())
                .build(),
            MessageIoBuilder::<Fir<A, N>>::new().build(),
            Fir { taps },
        )
    }
}

#[async_trait]
impl<const N: usize> SyncKernel for Fir<f32, N> {
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        if i.len() >= N {
            let n = std::cmp::min(i.len() + 1 - N, o.len());

            unsafe {
                for k in 0..n {
                    let mut sum = 0.0;
                    for t in 0..N {
                        sum = fadd_fast(
                            sum,
                            fmul_fast(*i.get_unchecked(k + t), *self.taps.get_unchecked(t)),
                        );
                    }
                    *o.get_unchecked_mut(k) = sum;
                }
            }

            if sio.input(0).finished() && n == i.len() + 1 - N {
                io.finished = true;
            }
        } else {
            if sio.input(0).finished() {
                io.finished = true;
            }
        }

        Ok(())
    }
}
