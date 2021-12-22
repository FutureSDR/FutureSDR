use crate::runtime::{Block, StreamIoBuilder, BlockMetaBuilder, MessageIoBuilder, AsyncKernel, WorkIo, StreamIo, MessageIo, BlockMeta};
use std::mem;
use num_complex::Complex;
use async_trait::async_trait;
use anyhow::Result;
use std::cmp;


pub struct QuadratureDemodulation {
    last: Complex<f32>
}

impl QuadratureDemodulation {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("QuadratureDemodulation").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<Complex<f32>>())
                .add_output("out", mem::size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self { last: Complex::<f32>::new(0.0, 0.0) }
        )
    }
}

#[async_trait]
impl AsyncKernel for QuadratureDemodulation {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<f32>();

        let n = cmp::min(i.len(), o.len());

        if (sio.input(0).finished()) & (n == i.len()) {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }
        
//        println!("{}, {}", i.len(), o.len());

        if n > 0  {
            for t in 0..n {
                let tmp = i[t] * self.last.conj();
                o[t] = f32::atan2(tmp.im, tmp.re);
                self.last = i[t].clone();
            }
        }

        sio.output(0).produce(n);
        sio.input(0).consume(n);

        Ok(())
    }
}
