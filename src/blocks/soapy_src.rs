use anyhow::{Context, Result};
use futures::FutureExt;
use num_complex::Complex;
use soapysdr::Direction::Rx;
use std::cmp;
use std::mem;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct SoapySource {
    dev: Option<soapysdr::Device>,
    stream: Option<soapysdr::RxStream<Complex<f32>>>,
}

impl SoapySource {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("SoapySource").blocking().build(),
            StreamIoBuilder::new()
                .add_stream_output("out", mem::size_of::<Complex<f32>>())
                .build(),
            MessageIoBuilder::new()
                .register_async_input(
                    "freq",
                    |block: &mut SoapySource,
                     _mio: &mut MessageIo<SoapySource>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::U32(ref f) = &p {
                                block.dev.as_mut().context("no dev")?.set_frequency(
                                    Rx,
                                    0,
                                    *f as f64,
                                    (),
                                )?;
                            }
                            Ok(p)
                        }
                        .boxed()
                    },
                )
                .build(),
            SoapySource {
                dev: None,
                stream: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for SoapySource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<Complex<f32>>();
        let stream = self.stream.as_mut().unwrap();
        let n = cmp::min(out.len(), stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        if let Ok(len) = stream.read(&[&mut out[..n]], 1_000_000) {
            sio.output(0).produce(len);
        }
        io.call_again = true;
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let channel: usize = 0;
        self.dev = Some(soapysdr::Device::new("")?);
        let dev = self.dev.as_ref().context("no dev")?;
        dev.set_frequency(Rx, channel, 100e6, ()).unwrap();
        dev.set_sample_rate(Rx, channel, 3.2e6).unwrap();
        dev.set_gain(Rx, channel, 34.0).unwrap();

        self.stream = Some(dev.rx_stream::<Complex<f32>>(&[channel]).unwrap());
        self.stream.as_mut().context("no stream")?.activate(None)?;

        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.stream
            .as_mut()
            .context("no stream")?
            .deactivate(None)?;
        Ok(())
    }
}

unsafe impl Sync for SoapySource {}

pub struct SoapySourceBuilder {}

impl SoapySourceBuilder {
    pub fn new() -> SoapySourceBuilder {
        SoapySourceBuilder {}
    }

    pub fn build(self) -> Block {
        SoapySource::new()
    }
}

impl Default for SoapySourceBuilder {
    fn default() -> Self {
        Self::new()
    }
}
