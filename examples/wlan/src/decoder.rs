use std::any::Any;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::warn;
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
                    warn!("decoder: canceling frame");
                }
                let frame_param = any.downcast_ref::<FrameParam>().unwrap();
                warn!("decoder frame param {:?}", frame_param);
            } else {
                input = &input[0..*index];
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
