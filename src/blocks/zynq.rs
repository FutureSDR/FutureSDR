use std::marker::PhantomData;

use xilinx_dma::AxiDmaAsync;
use xilinx_dma::DmaBuffer;

use crate::anyhow::Result;
use crate::runtime::buffer::zynq::BufferEmpty;
use crate::runtime::buffer::zynq::BufferFull;
use crate::runtime::buffer::zynq::ReaderH2D;
use crate::runtime::buffer::zynq::WriterD2H;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Zynq<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    dma_h2d: AxiDmaAsync,
    dma_d2h: AxiDmaAsync,
    dma_buffs: Vec<String>,
    output_buffers: Vec<BufferEmpty>,
    input_data: PhantomData<I>,
    output_data: PhantomData<O>,
}

impl<I, O> Zynq<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub fn new<S: Into<String>>(
        dma_h2d: String,
        dma_d2h: String,
        dma_buffs: Vec<S>,
    ) -> Result<Block> {
        assert!(dma_buffs.len() > 1);
        let dma_buffs = dma_buffs.into_iter().map(Into::into).collect();

        Ok(Block::new(
            BlockMetaBuilder::new("Zynq").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<I>())
                .add_output("out", std::mem::size_of::<O>())
                .build(),
            MessageIoBuilder::<Zynq<I, O>>::new().build(),
            Zynq {
                dma_h2d: AxiDmaAsync::new(&dma_h2d)?,
                dma_d2h: AxiDmaAsync::new(&dma_d2h)?,
                dma_buffs,
                output_buffers: Vec::new(),
                input_data: PhantomData,
                output_data: PhantomData,
            },
        ))
    }
}

#[inline]
fn o(sio: &mut StreamIo, id: usize) -> &mut WriterD2H {
    sio.output(id).try_as::<WriterD2H>().unwrap()
}

#[inline]
fn i(sio: &mut StreamIo, id: usize) -> &mut ReaderH2D {
    sio.input(id).try_as::<ReaderH2D>().unwrap()
}

#[async_trait]
impl<I, O> Kernel for Zynq<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn init(
        &mut self,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let len = self.dma_buffs.len();
        assert!(len > 1);

        for n in self.dma_buffs[..len / 2].iter() {
            self.output_buffers.push(BufferEmpty {
                buffer: DmaBuffer::new(n)?,
            });
        }

        for n in self.dma_buffs[len / 2..].iter() {
            i(sio, 0).submit(BufferEmpty {
                buffer: DmaBuffer::new(n)?,
            });
        }

        self.dma_h2d.reset();
        self.dma_d2h.reset();

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.output_buffers.extend(o(sio, 0).buffers());

        while !self.output_buffers.is_empty() {
            if let Some(BufferFull {
                buffer: inbuff,
                used_bytes,
            }) = i(sio, 0).get_buffer()
            {
                let outbuff = self.output_buffers.pop().unwrap().buffer;

                self.dma_h2d.start_h2d(&inbuff, used_bytes).await.unwrap();
                self.dma_d2h.start_d2h(&outbuff, used_bytes).await.unwrap();
                debug!("dma transfers started (bytes: {})", used_bytes);
                self.dma_d2h.wait_d2h().await.unwrap();

                i(sio, 0).submit(BufferEmpty { buffer: inbuff });
                o(sio, 0).submit(BufferFull {
                    buffer: outbuff,
                    used_bytes,
                });
            } else {
                break;
            }
        }

        if sio.input(0).finished() && !i(sio, 0).buffer_available() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct ZynqBuilder<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    dma_h2d: String,
    dma_d2h: String,
    dma_buffs: Vec<String>,
    _in: PhantomData<I>,
    _out: PhantomData<O>,
}

impl<I, O> ZynqBuilder<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    pub fn new<S: Into<String>>(
        dma_h2d: &str,
        dma_d2h: &str,
        dma_buffs: Vec<S>,
    ) -> ZynqBuilder<I, O> {
        let dma_buffs = dma_buffs.into_iter().map(Into::into).collect();
        ZynqBuilder {
            dma_h2d: dma_h2d.to_string(),
            dma_d2h: dma_d2h.to_string(),
            dma_buffs,
            _in: PhantomData,
            _out: PhantomData,
        }
    }

    pub fn build(self) -> Result<Block> {
        Zynq::<I, O>::new(self.dma_h2d, self.dma_d2h, self.dma_buffs)
    }
}
