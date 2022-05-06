use std::future::Future;
use std::pin::Pin;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::futures::FutureExt;
use futuresdr::log::{info, warn};
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIoBuilder;

pub struct Mac {}

impl Mac {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Mac").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::received)
                .build(),
            Mac {},
        )
    }

    fn check_crc(data: &[u8]) -> bool {
        let mut crc: u16 = 0;

        for b in data.iter() {
            for k in 0..8 {
                let bit = if b & (1 << k) != 0 {
                    1 ^ (crc & 1)
                } else {
                    crc & 1
                };
                crc >>= 1;
                if bit != 0 {
                    crc ^= 1 << 15;
                    crc ^= 1 << 10;
                    crc ^= 1 << 3;
                }
            }
        }
        crc == 0
    }

    fn received<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Mac>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            match p {
                Pmt::Blob(data) => {
                    if Self::check_crc(&data) {
                        info!("received frame, crc correct, payload length {}", data.len());
                        let l = data.len();
                        let s = String::from_iter(
                            data[7..l - 4]
                                .iter()
                                .map(|x| char::from(*x))
                                .map(|x| if x.is_ascii() { x } else { '.' })
                                .map(|x| {
                                    if ['\x0b', '\x0c', '\n', '\t', '\r'].contains(&x) {
                                        '.'
                                    } else {
                                        x
                                    }
                                }),
                        );
                        info!("{}", s);
                    } else {
                        info!("crc wrong");
                    }
                }
                _ => {
                    warn!("ZigBee Mac: received wrong PMT type");
                }
            }
            Ok(Pmt::Null)
        }
        .boxed()
    }
}

#[async_trait]
impl Kernel for Mac {}
