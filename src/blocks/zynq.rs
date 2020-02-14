use anyhow::{bail, Result};
use xilinx_dma::AxiDmaAsync;
use xilinx_dma::DmaBuffer;

use crate::runtime::buffer::zynq::BufferEmpty;
use crate::runtime::buffer::zynq::BufferFull;
use crate::runtime::buffer::zynq::ReaderH2D;
use crate::runtime::buffer::zynq::WriterD2H;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Zynq {
    dma_h2d: AxiDmaAsync,
    dma_d2h: AxiDmaAsync,
    dma_buffs: (String, String),
    buff_h2d: Option<BufferFull>,
    buff_d2h: Option<BufferEmpty>,
    read: u64,
}

impl Zynq {
    pub fn new(dma_h2d: String, dma_d2h: String, dma_buffs: (String, String)) -> Result<Block> {
        Ok(Block::new_async(
            BlockMetaBuilder::new("Zynq").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", 4)
                .add_stream_output("out", 4)
                .build(),
            MessageIoBuilder::<Zynq>::new().build(),
            Zynq {
                dma_h2d: AxiDmaAsync::new(&dma_h2d)?,
                dma_d2h: AxiDmaAsync::new(&dma_d2h)?,
                dma_buffs,
                buff_h2d: None,
                buff_d2h: None,
                read: 0,
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
impl AsyncKernel for Zynq {
    async fn init(
        &mut self,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        i(sio, 0).submit(BufferEmpty {
            buffer: DmaBuffer::new(&self.dma_buffs.0)?,
        });

        self.buff_d2h = Some(BufferEmpty {
            buffer: DmaBuffer::new(&self.dma_buffs.1)?,
        });

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for m in o(sio, 0).buffers().drain(..) {
            debug!("zynq: message in output buff");
            debug_assert!(self.buff_d2h.is_none());

            self.buff_d2h = Some(m);
        }

        for m in i(sio, 0).buffers().drain(..) {
            debug!("zynq: message in input buff");
            debug_assert!(self.buff_h2d.is_none());

            self.buff_h2d = Some(m);
        }

        if self.buff_h2d.is_some() && self.buff_d2h.is_some() {
            match (self.buff_h2d.take(), self.buff_d2h.take()) {
                (
                    Some(BufferFull {
                        buffer: inbuff,
                        used_bytes,
                    }),
                    Some(BufferEmpty { buffer: outbuff }),
                ) => {
                    self.dma_h2d.start_h2d(&inbuff, used_bytes).await.unwrap();
                    self.dma_d2h.start_d2h(&outbuff, used_bytes).await.unwrap();
                    debug!("dma transfers started");
                    self.dma_h2d.wait_h2d().await.unwrap();
                    self.dma_d2h.wait_d2h().await.unwrap();
                    self.read += (used_bytes / 4) as u64;
                    debug!("dma transfers completed");
                    i(sio, 0).submit(BufferEmpty { buffer: inbuff });
                    o(sio, 0).submit(BufferFull {
                        buffer: outbuff,
                        used_bytes,
                    });
                }
                _ => bail!("zynq failed to destructure buffers"),
            }
        }

        if sio.input(0).finished() && self.buff_h2d.is_none() {
            info!("zynq stopped. read {}", self.read);
            io.finished = true;
        }

        Ok(())
    }
}

pub struct ZynqBuilder {
    dma_h2d: String,
    dma_d2h: String,
    dma_buffs: (String, String),
}

impl ZynqBuilder {
    pub fn new(dma_h2d: &str, dma_d2h: &str, dma_buffs: (&str, &str)) -> ZynqBuilder {
        ZynqBuilder {
            dma_h2d: dma_h2d.to_string(),
            dma_d2h: dma_d2h.to_string(),
            dma_buffs: (dma_buffs.0.to_string(), dma_buffs.1.to_string()),
        }
    }

    pub fn build(self) -> Result<Block> {
        Zynq::new(self.dma_h2d, self.dma_d2h, self.dma_buffs)
    }
}
