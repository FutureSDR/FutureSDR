use crate::FrameParam;
use crate::Mcs;
use crate::Modulation;
use crate::ViterbiDecoder;
use crate::LONG;
use crate::POLARITY;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::info;
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

const INTERLEAVER_PATTERN: [usize; 48] = [
    0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45, 1, 4, 7, 10, 13, 16, 19, 22, 25,
    28, 31, 34, 37, 40, 43, 46, 2, 5, 8, 11, 14, 17, 20, 23, 26, 29, 32, 35, 38, 41, 44, 47,
];

struct Equalizer {
    h: [Complex32; 64],
    snr: f32,
}

impl Equalizer {
    fn new() -> Self {
        Equalizer {
            h: [Complex32::new(0.0, 0.0); 64],
            snr: 0.0,
        }
    }
    fn sync1(&mut self, s: &[Complex32; 64]) {
        // println!("{:?}", s);
        self.h.copy_from_slice(s);
    }
    fn sync2(&mut self, s: &[Complex32; 64]) {
        // println!("{:?}", s);
        let mut signal = 0.0f32;
        let mut noise = 0.0f32;
        for i in 6..=58 {
            if i == 32 {
                continue;
            }
            noise += (self.h[i] - s[i]).norm_sqr();
            signal += (self.h[i] + s[i]).norm_sqr();

            self.h[i] += s[i];
            self.h[i] /= LONG[i] + LONG[i];
        }
        self.snr = 10.0 * (signal / noise / 2.0).log10();
    }

    fn equalize(
        &mut self,
        input: &[Complex32; 64],
        output_symbols: &mut [Complex32; 48],
        output_bits: &mut [u8; 48],
        modulation: Modulation,
    ) {
        for (o, i) in (6..=58)
            .filter(|x| ![11, 25, 32, 39, 53].contains(x))
            .enumerate()
        {
            output_symbols[o] = input[i] / self.h[i];
            output_bits[o] = modulation.demap(&output_symbols[o]);
        }
    }

    fn snr(&self) -> f32 {
        self.snr
    }
}

#[derive(Debug)]
enum State {
    Sync1,
    Sync2,
    Signal,
    Copy(usize, usize, Modulation),
    Skip,
}

pub struct FrameEqualizer {
    equalizer: Equalizer,
    state: State,
    sym_in: [Complex32; 64],
    sym_out: [Complex32; 48],
    decoded_bits: [u8; 24],
    bits_out: [u8; 48],
    decoder: ViterbiDecoder,
}

impl FrameEqualizer {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FrameEqualizer").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<Complex32>())
                .add_output("out", std::mem::size_of::<u8>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                equalizer: Equalizer::new(),
                state: State::Skip,
                sym_in: [Complex32::new(0.0, 0.0); 64],
                sym_out: [Complex32::new(0.0, 0.0); 48],
                decoded_bits: [0; 24],
                bits_out: [0; 48],
                decoder: ViterbiDecoder::new(),
            },
        )
    }

    fn decode_signal_field(&mut self) -> Option<FrameParam> {
        let bits = self.bits_out;
        // info!("bits: {:?}", &bits);

        let mut deinterleaved = [0u8; 48];
        for i in 0..48 {
            deinterleaved[i] = bits[INTERLEAVER_PATTERN[i]];
        }
        // info!("deinterleaved: {:?}", &deinterleaved);

        self.decoder.decode(
            FrameParam::new(Mcs::Bpsk_1_2, 0),
            &deinterleaved,
            &mut self.decoded_bits,
        );
        let decoded_bits = self.decoded_bits;
        // info!("decoded: {:?}", &decoded_bits[0..24]);

        let mut r = 0;
        let mut bytes = 0;
        let mut parity = false;
        for i in 0..17 {
            parity ^= decoded_bits[i] > 0;

            if (i < 4) && (decoded_bits[i] > 0) {
                r |= 1 << i;
            }

            if (decoded_bits[i] > 0) && (i > 4) && (i < 17) {
                bytes |= 1 << (i - 5);
            }
        }

        if parity as u8 != decoded_bits[17] {
            return None;
        }

        match r {
            11 => Some(FrameParam::new(Mcs::Bpsk_1_2, bytes)),
            15 => Some(FrameParam::new(Mcs::Bpsk_3_4, bytes)),
            10 => Some(FrameParam::new(Mcs::Qpsk_1_2, bytes)),
            14 => Some(FrameParam::new(Mcs::Qpsk_3_4, bytes)),
            9 => Some(FrameParam::new(Mcs::Qam16_1_2, bytes)),
            13 => Some(FrameParam::new(Mcs::Qam16_3_4, bytes)),
            8 => Some(FrameParam::new(Mcs::Qam64_2_3, bytes)),
            12 => Some(FrameParam::new(Mcs::Qam64_3_4, bytes)),
            _ => {
                info!("signal: wrong encoding (r = {})", r);
                None
            }
        }
    }
}

#[async_trait]
impl Kernel for FrameEqualizer {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut input = sio.input(0).slice::<Complex32>();
        let out = sio.output(0).slice::<u8>();

        let tags = sio.input(0).tags();
        // info!("eq: input {} output {} tags {:?}", input.len(), out.len(), tags);
        if let Some((index, _freq)) = tags.iter().find_map(|x| match x {
            ItemTag {
                index,
                tag: Tag::NamedF32(n, f),
            } => {
                if n == "wifi_start" {
                    Some((index, f))
                } else {
                    None
                }
            }
            _ => None,
        }) {
            if *index == 0 {
                if !matches!(self.state, State::Skip) {
                    info!("frame equalizer: canceling frame");
                }
                self.state = State::Sync1;
            } else {
                input = &input[0..*index];
            }
        }

        let max_i = input.len() / 64;
        let max_o = out.len() / 48;
        let mut i = 0;
        let mut o = 0;

        while i < max_i {
            // copy symbol w/ fft shift
            for k in 0..64 {
                let m = (k + 32) % 64;
                self.sym_in[m] = input[i * 64 + k];
            }

            match self.state {
                State::Sync1 | State::Sync2 => {
                    let beta =
                        (self.sym_in[11] - self.sym_in[25] + self.sym_in[39] + self.sym_in[53])
                            .arg();
                    for i in 0..64 {
                        self.sym_in[i] *= Complex32::from_polar(1.0, -beta);
                    }
                }
                State::Signal => {
                    let p = POLARITY[0];
                    let beta = ((self.sym_in[11] * p)
                        + (self.sym_in[39] * p)
                        + (self.sym_in[25] * p)
                        + (self.sym_in[53] * -p))
                        .arg();
                    for i in 0..64 {
                        self.sym_in[i] *= Complex32::from_polar(1.0, -beta);
                    }
                }
                State::Copy(left, n, _) => {
                    let p = POLARITY[(n - left + 1) % 127];
                    let beta = ((self.sym_in[11] * p)
                        + (self.sym_in[39] * p)
                        + (self.sym_in[25] * p)
                        + (self.sym_in[53] * -p))
                        .arg();
                    for i in 0..64 {
                        self.sym_in[i] *= Complex32::from_polar(1.0, -beta);
                    }
                }
                _ => {}
            }

            // println!("equalizer state {:?}", self.state);

            // let b : Vec<u8> = (6..=58).filter(|i| *i != 32).map(|x| if self.sym_in[x].re > 0.0 { 0 } else { 1 }).collect();
            // info!("{:?} {:?}", &self.state, b);
            // for i in 0..64 {
            //     if (i == 32) || (i < 6) || (i > 58) {
            //         continue;
            //     }
            // }

            match &mut self.state {
                State::Sync1 => {
                    self.equalizer.sync1(&self.sym_in);
                    self.state = State::Sync2;
                    i += 1;
                }
                State::Sync2 => {
                    self.equalizer.sync2(&self.sym_in);
                    self.state = State::Signal;
                    i += 1;
                }
                State::Signal => {
                    self.equalizer.equalize(
                        &self.sym_in,
                        &mut self.sym_out,
                        &mut self.bits_out,
                        Modulation::Bpsk,
                    );
                    // info!("{:?}", &self.bits_out);
                    i += 1;
                    if let Some(frame) = self.decode_signal_field() {
                        // info!("signal field decoded {:?}, snr {}", &frame, self.equalizer.snr());

                        self.state = State::Copy(
                            frame.n_symbols(),
                            frame.n_symbols(),
                            frame.mcs().modulation(),
                        );
                        sio.output(0).add_tag(
                            o * 48,
                            Tag::NamedAny("wifi_start".to_string(), Box::new(frame)),
                        );
                    } else {
                        info!(
                            "signal field could not be decoded, snr {}",
                            self.equalizer.snr()
                        );
                        self.state = State::Skip;
                    }
                }
                State::Copy(mut n_sym, all_sym, modulation) => {
                    if o < max_o {
                        self.equalizer.equalize(
                            &self.sym_in,
                            &mut self.sym_out,
                            (&mut out[o * 48..(o + 1) * 48]).try_into().unwrap(),
                            *modulation,
                        );

                        i += 1;
                        o += 1;

                        n_sym -= 1;
                        if n_sym == 0 {
                            self.state = State::Skip;
                        } else {
                            self.state = State::Copy(n_sym, *all_sym, *modulation);
                        }
                    } else {
                        break;
                    }
                }
                State::Skip => {
                    i += 1;
                }
            }
        }

        sio.input(0).consume(i * 64);
        sio.output(0).produce(o * 48);

        if sio.input(0).finished() && i == max_i {
            io.finished = true;
        }

        Ok(())
    }
}
