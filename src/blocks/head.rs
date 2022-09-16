use std::cmp;
use std::ptr;

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

/// Copies only a given number of samples and stops.
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
/// use futuresdr::blocks::Head;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let head = fg.add_block(Head::<Complex<f32>>::new(1_000_000));
/// ```
pub struct Head<T: Send + 'static> {
    n_items: u64,
    _type: std::marker::PhantomData<T>,
}
impl<T: Send + 'static> Head<T> {
    pub fn new(n_items: u64) -> Block {
        Block::new(
            BlockMetaBuilder::new("Head").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<T>())
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            Head::<T> {
                n_items,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for Head<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();
        let item_size = std::mem::size_of::<T>();

        let mut m = cmp::min(self.n_items as usize, i.len() / item_size);
        m = cmp::min(m, o.len() / item_size);

        if m > 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m * item_size);
            }

            self.n_items -= m as u64;
            if self.n_items == 0 {
                io.finished = true;
            }
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        Ok(())
    }
}
