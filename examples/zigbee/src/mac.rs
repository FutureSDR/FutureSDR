use std::collections::VecDeque;
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
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

const MAX_FRAMES: usize = 128;
const MAX_FRAME_SIZE: usize = 127;
const FRAME_CONTROL: u16 = 0x8841;
const DESTINATION_PAN: u16 = 0x1aaa;
const DESTINATION_ADDRESS: u16 = 0xffff;
const SOURCE_ADDRESS: u16 = 0x3344;

pub struct Mac {
    tx_frames: VecDeque<Vec<u8>>,
    current_frame: [u8; 256],
    sequence_number: u8,
    current_index: usize,
    current_len: usize,
}

impl Mac {
    pub fn new() -> Block {
        let mut b = [0; 256];
        b[0] = 0x0;
        b[1] = 0x0;
        b[2] = 0x0;
        b[3] = 0xa7;
        b[4] = 0x0; // len
        b[5] = FRAME_CONTROL.to_le_bytes()[0];
        b[6] = FRAME_CONTROL.to_le_bytes()[1];
        b[7] = 0x0; // seq nr
        b[8] = DESTINATION_PAN.to_le_bytes()[0];
        b[9] = DESTINATION_PAN.to_le_bytes()[1];
        b[10] = DESTINATION_ADDRESS.to_le_bytes()[0];
        b[11] = DESTINATION_ADDRESS.to_le_bytes()[1];
        b[12] = SOURCE_ADDRESS.to_le_bytes()[0];
        b[13] = SOURCE_ADDRESS.to_le_bytes()[1];

        Block::new(
            BlockMetaBuilder::new("Mac").build(),
            StreamIoBuilder::new().add_output("out", 1).build(),
            MessageIoBuilder::new()
                .add_input("rx", Self::received)
                .add_input("tx", Self::transmit)
                .build(),
            Mac {
                tx_frames: VecDeque::new(),
                current_frame: b,
                sequence_number: 0,
                current_index: 0,
                current_len: 0,
            },
        )
    }

    fn calc_crc(data: &[u8]) -> u16 {
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
        crc
    }

    fn check_crc(data: &[u8]) -> bool {
        Self::calc_crc(data) == 0
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
                            data[9..l - 2]
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
                    warn!(
                        "ZigBee Mac: received wrong PMT type in RX callback (expected Pmt::Blob)"
                    );
                }
            }
            Ok(Pmt::Null)
        }
        .boxed()
    }

    fn transmit<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Mac>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            match p {
                Pmt::Blob(data) => {
                    if self.tx_frames.len() >= MAX_FRAMES {
                        warn!(
                            "ZigBee Mac: max number of frames already in TX queue ({}). Dropping.",
                            MAX_FRAMES
                        );
                    } else {
                        // 9 header + 2 crc
                        if data.len() > MAX_FRAME_SIZE - 11 {
                            warn!(
                                "ZigBee Mac: TX frame too large ({}, max {}). Dropping.",
                                data.len(),
                                MAX_FRAME_SIZE - 11
                            );
                        } else {
                            self.tx_frames.push_back(data);
                        }
                    }
                }
                _ => {
                    warn!(
                        "ZigBee Mac: received wrong PMT type in TX callback (expected Pmt::Blob)"
                    );
                }
            }
            Ok(Pmt::Null)
        }
        .boxed()
    }
}

#[async_trait]
impl Kernel for Mac {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<u8>();

        while !out.is_empty() {
            if self.current_len == 0 {
                if let Some(v) = self.tx_frames.pop_front() {
                    self.current_frame[4] = (v.len() + 11) as u8;
                    self.current_frame[7] = self.sequence_number;
                    self.sequence_number = self.sequence_number.wrapping_add(1);
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            v.as_ptr(),
                            self.current_frame.as_mut_ptr().add(14),
                            v.len(),
                        );
                    }

                    let crc = Self::calc_crc(&self.current_frame[5..14 + v.len()]);
                    self.current_frame[14 + v.len()] = crc.to_le_bytes()[0];
                    self.current_frame[15 + v.len()] = crc.to_le_bytes()[1];

                    // 4 preamble + 1 len + 9 header + 2 crc
                    self.current_len = v.len() + 16;
                    self.current_index = 0;
                    sio.output(0).add_tag(0, Tag::Id(self.current_len as u64));
                    info!("sending frame, len {}", self.current_len);
                    info!("{:?}", &self.current_frame[0..self.current_len]);
                } else {
                    break;
                }
            } else {
                let n = std::cmp::min(out.len(), self.current_len - self.current_index);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.current_frame.as_ptr().add(self.current_index),
                        out.as_mut_ptr(),
                        n,
                    );
                }

                sio.output(0).produce(n);
                self.current_index += n;

                if self.current_index == self.current_len {
                    self.current_len = 0;
                }
            }
        }

        Ok(())
    }
}
