use std::cmp::min;
use std::marker::PhantomData;

use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Stream Duplicator
#[derive(Block)]
pub struct StreamDuplicator<T: Send> {
    num_out: usize,
    phantom: PhantomData<T>,
}

impl<T> StreamDuplicator<T>
where
    T: Copy + Send + Sync + 'static,
{
    /// Create Stream Duplicator.
    pub fn new(num_outputs: usize) -> TypedBlock<Self> {
        let mut sio = StreamIoBuilder::new().add_input::<T>("in");
        for i in 0..num_outputs {
            sio = sio.add_output::<T>(&format!("out{}", i));
        }
        TypedBlock::new(
            sio.build(),
            Self {
                num_out: num_outputs,
                phantom: PhantomData,
            },
        )
    }
}

#[doc(hidden)]
impl<T: Copy + Send + Sync + 'static> Kernel for StreamDuplicator<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<T>();
        let nitem_to_consume = input.len();
        let n_items_to_produce = sio
            .outputs_mut()
            .iter_mut()
            .map(|x| x.slice::<T>().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, nitem_to_consume);
        if nitem_to_process > 0 {
            for j in 0..self.num_out {
                let out = sio.output(j).slice::<T>();
                out[..nitem_to_process].copy_from_slice(&input[..nitem_to_process]);
                sio.output(j).produce(nitem_to_process);
            }
            sio.input(0).consume(nitem_to_process);
        }
        if nitem_to_consume - nitem_to_process == 0 && sio.input(0).finished() {
            io.finished = true;
        }
        Ok(())
    }
}
