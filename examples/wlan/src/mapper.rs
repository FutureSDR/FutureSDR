use crate::FrameParam;
use crate::Mcs;
use crate::Modulation;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
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
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::WorkIo;

/// Maximum number of frames to queue for transmission
const MAX_FRAMES: usize = 1000;

pub struct Mapper {
    signal: [u8; 24],
    signal_encoded: [u8; 48],
    signal_interleaved: [u8; 48],
    current_mod: Modulation,
    index: usize,
}

impl Mapper {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Mapper").build(),
            StreamIoBuilder::new()
                .add_input("in", 1)
                .add_output("out", 1)
                .build(),
            MessageIoBuilder::new().build(),
            Mapper {
                signal: [0; 24],
                signal_encoded: [0; 48],
                signal_interleaved: [0; 48],
                current_mod: Modulation::Bpsk,
                index: 0,
            },
        )
    }

    #[inline(always)]
    fn get_bit(data: u8, bit: usize) -> u8 {
        if data & (1 << bit) > 0 { 1 } else { 0 }
    }
    #[inline(always)]
    fn get_bit_usize(data: usize, bit: usize) -> u8 {
        if data & (1 << bit) > 0 { 1 } else { 0 }
    }

    fn generate_signal_field(&mut self, frame: &FrameParam) {
        let length = frame.psdu_size();
        let rate = frame.mcs().rate_field();

        // first 4 bits represent the modulation and coding scheme
        self.signal[0] = Self::get_bit(rate, 3);
        self.signal[1] = Self::get_bit(rate, 2);
        self.signal[2] = Self::get_bit(rate, 1);
        self.signal[3] = Self::get_bit(rate, 0);
        // 5th bit is reserved and must be set to 0
        self.signal[4] = 0;
        // then 12 bits represent the length
        self.signal[5] = Self::get_bit_usize(length, 0);
        self.signal[6] = Self::get_bit_usize(length, 1);
        self.signal[7] = Self::get_bit_usize(length, 2);
        self.signal[8] = Self::get_bit_usize(length, 3);
        self.signal[9] = Self::get_bit_usize(length, 4);
        self.signal[10] = Self::get_bit_usize(length, 5);
        self.signal[11] = Self::get_bit_usize(length, 6);
        self.signal[12] = Self::get_bit_usize(length, 7);
        self.signal[13] = Self::get_bit_usize(length, 8);
        self.signal[14] = Self::get_bit_usize(length, 9);
        self.signal[15] = Self::get_bit_usize(length, 10);
        self.signal[16] = Self::get_bit_usize(length, 11);
        // 18-th bit is the parity bit for the first 17 bits
        let sum : u8 = self.signal[0..17].iter().sum();
        self.signal[17] = sum % 2;

        // encode
        let mut state = 0;
        for i in 0..24 {
            state = ((state << 1) & 0x7e) | self.signal[i];
            self.signal_encoded[i * 2] = (state & 0o155).count_ones() as u8 % 2;
            self.signal_encoded[i * 2 + 1] = (state & 0o117).count_ones() as u8 % 2;
        }

        // interleave 
        const INTERLEAVER_PATTERN: [usize; 48] = [
            0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45, 1, 4, 7, 10, 13, 16, 19, 22, 25,
            28, 31, 34, 37, 40, 43, 46, 2, 5, 8, 11, 14, 17, 20, 23, 26, 29, 32, 35, 38, 41, 44, 47,
        ];

        for i in 0..48 {
            self.signal_interleaved[INTERLEAVER_PATTERN[i]] = self.signal_encoded[i];
        }

        info!("signal param {:?}\nsig: {:?}\nbits: {:?}", frame, &self.signal, &self.signal_interleaved);
    }
}

#[async_trait]
impl Kernel for Mapper {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {

        let mut input = sio.input(0).slice::<u8>();
        let output = sio.output(0).slice::<u8>();
        if output.len() < 64 {
            return Ok(());
        }

        let tags = sio.input(0).tags();
        if let Some((index, frame)) = tags.iter().find_map(|x| match x {
            ItemTag {
                index,
                tag: Tag::NamedAny(n, any),
            } => {
                if n == "wifi_start" {
                    any.downcast_ref::<FrameParam>().map(|x| (index, x))
                } else {
                    None
                }
            }
            _ => None,
        }) {
            if *index == 0 {
                self.generate_signal_field(frame);
            } else {
                input = &input[0..*index];
            }
        }









        // loop {
        //     let out = sio.output(0).slice::<u8>();
        //     if out.is_empty() {
        //         break;
        //     }

        //     if self.current_len == 0 {
        //         if let Some((data, mcs)) = self.tx_frames.pop_front() {
        //             self.current_len = self.encode(&data, mcs);
        //             self.current_index = 0;
        //             sio.output(0).add_tag(
        //                 0,
        //                 Tag::NamedAny("wifi_start".to_string(), Box::new((self.current_len, mcs))),
        //             );
        //         } else {
        //             break;
        //         }
        //     } else {
        //         let n = std::cmp::min(out.len(), self.current_len - self.current_index);
        //         unsafe {
        //             std::ptr::copy_nonoverlapping(
        //                 self.symbols.as_ptr().add(self.current_index),
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
