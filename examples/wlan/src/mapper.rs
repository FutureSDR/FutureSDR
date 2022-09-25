use crate::FrameParam;
use crate::Modulation;
use crate::POLARITY;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
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
                .add_output("out", std::mem::size_of::<Complex32>())
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
        u8::from(data & (1 << bit) > 0)
    }
    #[inline(always)]
    fn get_bit_usize(data: usize, bit: usize) -> u8 {
        u8::from(data & (1 << bit) > 0)
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
        let sum: u8 = self.signal[0..17].iter().sum();
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
            0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45, 1, 4, 7, 10, 13, 16, 19,
            22, 25, 28, 31, 34, 37, 40, 43, 46, 2, 5, 8, 11, 14, 17, 20, 23, 26, 29, 32, 35, 38,
            41, 44, 47,
        ];

        for i in 0..48 {
            self.signal_interleaved[INTERLEAVER_PATTERN[i]] = self.signal_encoded[i];
        }
    }

    fn map(input: &[u8; 48], output: &mut [Complex32; 64], modulation: Modulation, index: usize) {
        // dc
        output[32] = Complex32::new(0.0, 0.0);
        // guard
        for i in (0..6).chain(59..64) {
            output[i] = Complex32::new(0.0, 0.0);
        }
        // pilots
        for i in [11, 25, 39] {
            output[i] = POLARITY[index];
        }
        output[53] = -POLARITY[index];
        // data
        for (i, c) in (6..11)
            .chain(12..25)
            .chain(26..32)
            .chain(33..39)
            .chain(40..53)
            .chain(54..59)
            .enumerate()
        {
            // debug!("data {} {} mapped {}", i, c, modulation.map(input[i]));
            output[c] = modulation.map(input[i]);
        }
    }
}

#[async_trait]
impl Kernel for Mapper {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut input = sio.input(0).slice::<u8>();
        let output = sio.output(0).slice::<Complex32>();
        if output.len() < 64 {
            return Ok(());
        }

        let mut o = 0;

        let tags = sio.input(0).tags().clone();
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
                Self::map(
                    &self.signal_interleaved,
                    (&mut output[0..64]).try_into().unwrap(),
                    Modulation::Bpsk,
                    0,
                );
                o += 1;
                sio.output(0).add_tag(
                    0,
                    Tag::NamedUsize("wifi_start".to_string(), frame.n_symbols() + 1),
                );
                self.current_mod = frame.mcs().modulation();
                self.index = 1;
                input = &input[0..std::cmp::min(input.len(), frame.n_symbols() * 48)];
            } else {
                input = &input[0..*index];
            }
        }

        let n = std::cmp::min(input.len() / 48, (output.len() / 64) - o);

        for i in 0..n {
            Self::map(
                (&input[i * 48..(i + 1) * 48]).try_into().unwrap(),
                (&mut output[(i + o) * 64..(i + o + 1) * 64])
                    .try_into()
                    .unwrap(),
                self.current_mod,
                self.index,
            );
            self.index += 1;
        }

        sio.input(0).consume(n * 48);
        sio.output(0).produce((n + o) * 64);

        if sio.input(0).finished() && n == input.len() * 48 {
            io.finished = true;
        }

        Ok(())
    }
}
