use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::warn;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

use crate::Mcs;
use crate::MAX_ENCODED_BITS;
use crate::MAX_PSDU_SIZE;
use crate::MAX_SYM;
use crate::{FrameParam, ViterbiDecoder};

pub struct Decoder {
    frame_complete: bool,
    frame_param: FrameParam,
    decoder: ViterbiDecoder,
    copied: usize,
    rx_symbols: [u8; 48 * MAX_SYM],
    rx_bits: [u8; MAX_ENCODED_BITS],
    deinterleaved_bits: [u8; MAX_ENCODED_BITS],
    decoded_bits: [u8; MAX_ENCODED_BITS],
    out_bytes: [u8; MAX_PSDU_SIZE + 2], // 2 for signal field
}

impl Decoder {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Decoder").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<u8>())
                .build(),
            MessageIoBuilder::new()
                .add_output("rx_frames")
                .add_output("rftap")
                .build(),
            Self {
                frame_complete: true,
                frame_param: FrameParam::new(Mcs::Bpsk_1_2, 0),
                decoder: ViterbiDecoder::new(),
                copied: 0,
                rx_symbols: [0; 48 * MAX_SYM],
                rx_bits: [0; MAX_ENCODED_BITS],
                deinterleaved_bits: [0; MAX_ENCODED_BITS],
                decoded_bits: [0; MAX_ENCODED_BITS],
                out_bytes: [0; MAX_PSDU_SIZE + 2], // 2 for signal field
            },
        )
    }
    fn deinterleave(&mut self) {
        let n_cbps = self.frame_param.mcs().n_cbps();
        let n_bpsc = self.frame_param.mcs().modulation().n_bpsc();
        let mut first = vec![0usize; n_cbps];
        let mut second = vec![0usize; n_cbps];
        let s = std::cmp::max(n_bpsc / 2, 1);

        for j in 0..n_cbps {
            first[j] = s * (j / s) + ((j + (16 * j / n_cbps)) % s);
        }
        for i in 0..n_cbps {
            second[i] = 16 * i - (n_cbps - 1) * (16 * i / n_cbps);
        }

        for i in 0..self.frame_param.n_symbols() {
            for k in 0..n_cbps {
                self.deinterleaved_bits[i * n_cbps + second[first[k]]] =
                    self.rx_bits[i * n_cbps + k];
            }
        }
    }

    fn decode(&mut self) -> bool {
        let syms = self.frame_param.n_symbols();
        let bpsc = self.frame_param.mcs().modulation().n_bpsc();
        for i in 0..syms * 48 {
            for k in 0..bpsc {
                self.rx_bits[i * bpsc + k] = u8::from((self.rx_symbols[i] & (1 << k)) > 0);
            }
        }
        // println!("rx_symbols: {:?}", &self.rx_symbols[0..syms * 48]);
        // println!("rx_bits: {:?}", &self.rx_bits[0..syms * 48 * bpsc]);

        self.deinterleave();
        self.decoder.decode(
            self.frame_param.clone(),
            &self.deinterleaved_bits,
            &mut self.decoded_bits,
        );
        self.descramble();

        let crc = crc32fast::hash(&self.out_bytes[2..self.frame_param.psdu_size() + 2]);
        crc == 558161692
    }

    fn descramble(&mut self) {
        let decoded_bits = &self.decoded_bits;

        let mut state = 0;
        self.out_bytes[0..self.frame_param.psdu_size() + 2].fill(0);

        for i in 0..7 {
            if decoded_bits[i] > 0 {
                state |= 1 << (6 - i);
            }
        }

        self.out_bytes[0] = state;

        let mut feedback;
        let mut bit;

        for i in 7..self.frame_param.psdu_size() * 8 + 16 {
            feedback = u8::from((state & 64) > 0) ^ u8::from((state & 8) > 0);
            bit = feedback ^ (decoded_bits[i] & 1);
            self.out_bytes[i / 8] |= bit << (i % 8);
            state = ((state << 1) & 0x7e) | feedback;
        }
    }
}

#[async_trait]
impl Kernel for Decoder {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
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

        // println!("decoder: input len {}, complete {}, copied {}, frame {:?}, tags {:?}", input.len(), self.frame_complete, self.copied, self.frame_param, tags);

        let max_i = input.len() / 48;
        let mut i = 0;

        // println!("decoder input: {:?}", &input[0..max_i * 48]);

        while i < max_i {
            if self.copied < self.frame_param.n_symbols() {
                // println!("copying {} of {}", self.copied, self.frame_param.n_symbols());
                self.rx_symbols[(self.copied * 48)..((self.copied + 1) * 48)]
                    .copy_from_slice(&input[(i * 48)..((i + 1) * 48)]);
            }

            i += 1;
            self.copied += 1;

            if self.copied == self.frame_param.n_symbols() {
                self.frame_complete = true;

                if self.decode() {
                    // println!(
                    //     "decoded: {:?}",
                    //     &self.out_bytes[0..self.frame_param.psdu_size() + 2]
                    // );
                    let mut blob = vec![0; self.frame_param.psdu_size() - 4];
                    blob.copy_from_slice(&self.out_bytes[2..self.frame_param.psdu_size() - 2]);

                    let mut rftap = vec![0; blob.len() + 12];
                    rftap[0..4].copy_from_slice("RFta".as_bytes());
                    rftap[4..6].copy_from_slice(&3u16.to_le_bytes());
                    rftap[6..8].copy_from_slice(&1u16.to_le_bytes());
                    rftap[8..12].copy_from_slice(&105u32.to_le_bytes());
                    rftap[12..].copy_from_slice(&blob);
                    mio.output_mut(0).post(Pmt::Blob(blob)).await;
                    mio.output_mut(1).post(Pmt::Blob(rftap)).await;
                }

                i = max_i;
                break;
            }
        }

        sio.input(0).consume(i * 48);
        if sio.input(0).finished() && i == max_i {
            mio.output_mut(0).post(Pmt::Null).await;
            io.finished = true;
        }

        Ok(())
    }
}
