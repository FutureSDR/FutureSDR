use std::any::Any;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::{info, warn};
use futuresdr::runtime::tag::Downcast;
use futuresdr::runtime::tag::TagAny;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

use crate::FrameParam;
use crate::Mcs;
use crate::MAX_ENCODED_BITS;
use crate::MAX_PSDU_SIZE;
use crate::MAX_SYM;

pub struct Decoder {
    frame_complete: bool,
    frame_param: FrameParam,
    copied: usize,
    rx_symbols: [u8; 48 * MAX_SYM],
    rx_bits: [u8; MAX_ENCODED_BITS],
    deinterleaved_bits: [u8; MAX_ENCODED_BITS],
    out_bytes: [u8; MAX_PSDU_SIZE + 2], // 2 for signal field
}

impl Decoder {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Decoder").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<u8>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                frame_complete: true,
                frame_param: FrameParam {
                    mcs: Mcs::Bpsk_1_2,
                    bytes: 0,
                },
                copied: 0,
                rx_symbols: [0; 48 * MAX_SYM],
                rx_bits: [0; MAX_ENCODED_BITS],
                deinterleaved_bits: [0; MAX_ENCODED_BITS],
                out_bytes: [0; MAX_PSDU_SIZE + 2], // 2 for signal field
            },
        )
    }

    fn decode(&mut self) -> bool {
        false
    }
}

#[async_trait]
impl Kernel for Decoder {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut input = sio.input(0).slice::<u8>();

        let tags = sio.input(0).tags();
        if let Some((index, any)) = tags.iter().find_map(|x| match x {
            ItemTag {
                index,
                tag: Tag::NamedAny(n, any),
            } => {
                if n == "wifi_start" {
                    Some((index, any))
                } else {
                    None
                }
            }
            _ => None,
        }) {
            if *index == 0 {
                if !self.frame_complete {
                    warn!("decoder: previous frame not complete, canceling.");
                }
                let frame_param = any.downcast_ref::<FrameParam>().unwrap();
                if frame_param.n_symbols() <= MAX_SYM && frame_param.psdu_size() <= MAX_PSDU_SIZE {
                    self.frame_param = frame_param.clone();
                    self.copied = 0;
                    self.frame_complete = false;
                } else {
                    warn!("decoder: frame too large, dropping. ({:?})", frame_param);
                }
            } else {
                input = &input[0..*index];
            }
        }

        println!("decoder: input len {}, complete {}, copied {}, frame {:?}, tags {:?}", input.len(), self.frame_complete, self.copied, self.frame_param, tags);

        let max_i = input.len() / 48;
        let mut i = 0;

        while i < max_i {
            if self.copied < self.frame_param.n_symbols() {
                println!("copying {} of {}", self.copied, self.frame_param.n_symbols());
                self.rx_symbols[(self.copied * 48)..((self.copied + 1) * 48)].copy_from_slice(&input[(i * 48)..((i + 1) * 48)]);
            }

            i += 1;
            self.copied += 1;

            if self.copied == self.frame_param.n_symbols() {
                self.frame_complete = true;

                info!("decoding");
                self.decode();

                i = max_i;
                break;
            }
        }

        sio.input(0).consume(i * 48);
        if sio.input(0).finished() && i == max_i {
            io.finished = true;
        }

        Ok(())
    }
}
