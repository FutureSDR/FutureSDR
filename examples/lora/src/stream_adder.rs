use futuresdr::anyhow::Result;
use futuresdr::macros::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;
use std::cmp::min;
use std::marker::PhantomData;
use std::ops::Add;

pub struct StreamAdder<T> {
    num_in: usize,
    phantom: PhantomData<T>,
}

impl<T> StreamAdder<T>
where
    T: Copy + Send + Sync + Add<Output = T> + 'static,
{
    pub fn new(num_inputs: usize) -> Block {
        let mut sio = StreamIoBuilder::new();
        for i in 0..num_inputs {
            sio = sio.add_input::<T>(&format!("in{}", i));
        }
        sio = sio.add_output::<T>("out");
        Block::new(
            BlockMetaBuilder::new("StreamAdder").build(),
            sio.build(),
            MessageIoBuilder::new().build(),
            StreamAdder::<T> {
                num_in: num_inputs,
                phantom: PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Copy + Send + Sync + Add<Output = T> + 'static> Kernel for StreamAdder<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let out: &mut [T] = sio.output(0).slice::<T>();
        let n_items_to_produce = out.len();
        let nitem_to_consume = sio
            .inputs_mut()
            .iter_mut()
            .map(|x| x.slice::<T>().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, nitem_to_consume);
        if nitem_to_process > 0 {
            out[..nitem_to_process].copy_from_slice(&sio.input(0).slice::<T>()[..nitem_to_process]);
            sio.input(0).consume(nitem_to_process);
            for j in 1..self.num_in {
                let input: &[T] = sio.input(j).slice::<T>();
                out[..nitem_to_process]
                    .iter_mut()
                    .zip(&input[..nitem_to_process])
                    .for_each(|(x, &y)| *x = *x + y);
                sio.input(j).consume(nitem_to_process);
            }
            sio.output(0).produce(nitem_to_process);
        }
        if sio
            .inputs_mut()
            .iter_mut()
            .any(|buf| buf.finished() && buf.slice::<T>().len() - nitem_to_process == 0)
        {
            io.finished = true;
        }
        Ok(())
    }
}
