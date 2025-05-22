use xilinx_dma::AxiDmaAsync;
use xilinx_dma::DmaBuffer;

use crate::prelude::*;
use crate::runtime::buffer::zynq::BufferEmpty;
use crate::runtime::buffer::zynq::BufferFull;
use crate::runtime::buffer::zynq::D2HWriter;
use crate::runtime::buffer::zynq::H2DReader;
use crate::runtime::buffer::CpuSample;

/// Interface Zynq FPGA w/ AXI DMA (async mode).
///
/// # Stream Inputs
///
/// `in`: Zynq custom buffer
///
/// # Stream Outputs
///
/// `out`: Zynq custom buffer
#[derive(Block)]
pub struct Zynq<I, O>
where
    I: CpuSample,
    O: CpuSample,
{
    #[input]
    input: H2DReader<I>,
    #[output]
    output: D2HWriter<O>,
    dma_h2d: AxiDmaAsync,
    dma_d2h: AxiDmaAsync,
    dma_buffs: Vec<String>,
    output_buffers: Vec<BufferEmpty>,
}

impl<I, O> Zynq<I, O>
where
    I: CpuSample,
    O: CpuSample,
{
    /// Create Zynq block
    pub fn new<S: Into<String>>(
        dma_h2d: impl AsRef<str>,
        dma_d2h: impl AsRef<str>,
        dma_buffs: Vec<S>,
    ) -> Result<Self> {
        assert!(dma_buffs.len() > 1);
        let dma_buffs = dma_buffs.into_iter().map(Into::into).collect();

        Ok(Self {
            input: H2DReader::new(),
            output: D2HWriter::new(),
            dma_h2d: AxiDmaAsync::new(dma_h2d.as_ref())?,
            dma_d2h: AxiDmaAsync::new(dma_d2h.as_ref())?,
            dma_buffs,
            output_buffers: Vec::new(),
        })
    }
}

#[doc(hidden)]
impl<I, O> Kernel for Zynq<I, O>
where
    I: CpuSample,
    O: CpuSample,
{
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let len = self.dma_buffs.len();
        assert!(len > 1);

        for n in self.dma_buffs[..len / 2].iter() {
            self.output_buffers.push(BufferEmpty {
                buffer: DmaBuffer::new(n)?,
            });
        }

        for n in self.dma_buffs[len / 2..].iter() {
            self.input.submit(BufferEmpty {
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.output_buffers.extend(self.output.buffers());

        while !self.output_buffers.is_empty() {
            if let Some(BufferFull {
                buffer: inbuff,
                used_bytes,
            }) = self.input.get_buffer()
            {
                let outbuff = self.output_buffers.pop().unwrap().buffer;

                self.dma_h2d.start_h2d(&inbuff, used_bytes).await.unwrap();
                self.dma_d2h.start_d2h(&outbuff, used_bytes).await.unwrap();
                debug!("dma transfers started (bytes: {})", used_bytes);
                self.dma_d2h.wait_d2h().await.unwrap();

                self.input.submit(BufferEmpty { buffer: inbuff });
                self.output.submit(BufferFull {
                    buffer: outbuff,
                    used_bytes,
                });
            } else {
                break;
            }
        }

        if self.input.finished() && !self.input.buffer_available() {
            io.finished = true;
        }

        Ok(())
    }
}
