use std::cmp::min;
use std::marker::PhantomData;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Stream Deinterleaver
pub struct StreamDeinterleaver<T> {
    num_channels: usize,
    phantom: PhantomData<T>,
}

impl<T> StreamDeinterleaver<T>
where
    T: Copy + Send + Sync + 'static,
{
    /// Stream Deinterleaver
    pub fn new(num_channels: usize) -> TypedBlock<Self> {
        let mut sio = StreamIoBuilder::new().add_input::<T>("in");
        for i in 0..num_channels {
            sio = sio.add_output::<T>(&format!("out{i}"));
        }
        TypedBlock::new(
            BlockMetaBuilder::new("StreamDeinterleaver").build(),
            sio.build(),
            MessageIoBuilder::new().build(),
            Self {
                num_channels,
                phantom: PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Copy + Send + Sync + 'static> Kernel for StreamDeinterleaver<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<T>();
        let n_items_to_consume = input.len();
        let n_items_to_produce = sio
            .outputs_mut()
            .iter_mut()
            .map(|x| x.slice::<T>().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, n_items_to_consume / self.num_channels);
        if nitem_to_process > 0 {
            for j in 0..self.num_channels {
                let out = sio.output(j).slice::<T>();
                for (out_slot, &in_item) in out[0..nitem_to_process].iter_mut().zip(
                    input[j..]
                        .iter()
                        .step_by(self.num_channels)
                        .take(nitem_to_process),
                ) {
                    *out_slot = in_item;
                }
                sio.output(j).produce(nitem_to_process);
            }
            sio.input(0).consume(nitem_to_process * self.num_channels);
        }
        if n_items_to_consume - (nitem_to_process * self.num_channels) < self.num_channels
            && sio.input(0).finished()
        {
            io.finished = true;
        }
        Ok(())
    }
}
