use crate::Mcs;
use crate::MAX_PAYLOAD_SIZE;
use crate::MAX_PSDU_SIZE;

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
use futuresdr::runtime::WorkIo;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;

/// Maximum number of frames to queue for transmission
const MAX_FRAMES: usize = 1000;

pub struct Mac {
    tx_frames: VecDeque<(Vec<u8>, Mcs)>,

    current_frame: [u8; MAX_PSDU_SIZE],
    current_index: usize,
    current_len: usize,
    current_mcs: Mcs,

    default_mcs: Mcs,
    sequence_number: u16,
    src_mac: [u8; 6],
    dst_mac: [u8; 6],
    bss_mac: [u8; 6],
}

impl Mac {
    pub fn new(src_mac: [u8; 6], dst_mac: [u8; 6], bss_mac: [u8; 6], default_mcs: Mcs) -> Block {
        Block::new(
            BlockMetaBuilder::new("Mac").build(),
            StreamIoBuilder::new().add_output("out", 1).build(),
            MessageIoBuilder::new()
                .add_input("tx", Self::transmit)
                .build(),
            Mac {
                tx_frames: VecDeque::new(),

                current_frame: [0; MAX_PSDU_SIZE],
                current_index: 0,
                current_len: 0,
                current_mcs: Mcs::Bpsk_1_2,

                default_mcs,
                sequence_number: 0,
                src_mac,
                dst_mac,
                bss_mac,
            },
        )
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
                            "WLAN Mac: max number of frames already in TX queue ({}). Dropping.",
                            MAX_FRAMES
                        );
                    } else {
                        if data.len() > MAX_PAYLOAD_SIZE {
                            warn!(
                                "WLAN Mac: TX frame too large ({}, max {}). Dropping.",
                                data.len(),
                                MAX_PAYLOAD_SIZE
                            );
                        } else {
                            info!("QUEUED BLOB");
                            self.tx_frames.push_back((data, self.default_mcs));
                        }
                    }
                }
                Pmt::Any(a) => {
                    if let Some((data, mcs)) = a.downcast_ref::<(Vec<u8>, Mcs)>() {
                        let data = data.clone();
                        if self.tx_frames.len() >= MAX_FRAMES {
                            warn!(
                                "WLAN Mac: max number of frames already in TX queue ({}). Dropping.",
                                MAX_FRAMES
                            );
                        } else {
                            if data.len() > MAX_PAYLOAD_SIZE {
                                warn!(
                                    "WLAN Mac: TX frame too large ({}, max {}). Dropping.",
                                    data.len(),
                                    MAX_PAYLOAD_SIZE
                                );
                            } else {
                                info!("QUEUED ANY {:?}", mcs);
                                self.tx_frames.push_back((data, *mcs));
                            }
                        }
                    }
                }
                x => {
                    warn!("WLAN Mac: received wrong PMT type in TX callback. {:?}", x);
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
        // loop {
        //     let out = sio.output(0).slice::<u8>();
        //     if out.is_empty() {
        //         break;
        //     }

        //     if self.current_len == 0 {
        //         if let Some(v) = self.tx_frames.pop_front() {
        //             self.current_frame[4] = (v.len() + 11) as u8;
        //             self.current_frame[7] = self.sequence_number;
        //             self.sequence_number = self.sequence_number.wrapping_add(1);
        //             unsafe {
        //                 std::ptr::copy_nonoverlapping(
        //                     v.as_ptr(),
        //                     self.current_frame.as_mut_ptr().add(14),
        //                     v.len(),
        //                 );
        //             }

        //             let crc = Self::calc_crc(&self.current_frame[5..14 + v.len()]);
        //             self.current_frame[14 + v.len()] = crc.to_le_bytes()[0];
        //             self.current_frame[15 + v.len()] = crc.to_le_bytes()[1];

        //             // 4 preamble + 1 len + 9 header + 2 crc
        //             self.current_len = v.len() + 16;
        //             self.current_index = 0;
        //             sio.output(0).add_tag(0, Tag::Id(self.current_len as u64));
        //             debug!("sending frame, len {}", self.current_len);
        //             self.n_sent += 1;
        //             debug!("{:?}", &self.current_frame[0..self.current_len]);
        //         } else {
        //             break;
        //         }
        //     } else {
        //         let n = std::cmp::min(out.len(), self.current_len - self.current_index);
        //         unsafe {
        //             std::ptr::copy_nonoverlapping(
        //                 self.current_frame.as_ptr().add(self.current_index),
        //                 out.as_mut_ptr(),
        //                 n,
        //             );
        //         }

        //         sio.output(0).produce(n);
        //         self.current_index += n;

        //         if self.current_index == self.current_len {
        //             self.current_len = 0;
        //         }
        //     }
        // }

        Ok(())
    }
}
