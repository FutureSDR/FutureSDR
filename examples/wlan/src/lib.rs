#![allow(clippy::new_ret_no_self)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::excessive_precision)]
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::StreamInput;
use futuresdr::runtime::StreamOutput;

mod channels;
pub use channels::channel_to_freq;
pub use channels::parse_channel;

mod decoder;
pub use decoder::Decoder;

mod delay;
pub use delay::Delay;

mod encoder;
pub use encoder::Encoder;

mod frame_equalizer;
pub use frame_equalizer::FrameEqualizer;

mod mac;
pub use mac::Mac;

mod mapper;
pub use mapper::Mapper;

mod moving_average;
pub use moving_average::MovingAverage;

mod prefix;
pub use prefix::Prefix;

mod sync_long;
pub use sync_long::SyncLong;

mod sync_short;
pub use sync_short::SyncShort;

mod viterbi_decoder;
pub use viterbi_decoder::ViterbiDecoder;

pub const MAX_PAYLOAD_SIZE: usize = 1500;
pub const MAX_PSDU_SIZE: usize = MAX_PAYLOAD_SIZE + 28; // MAC, CRC
pub const MAX_SYM: usize = ((16 + 8 * MAX_PSDU_SIZE + 6) / 24) + 1;
pub const MAX_ENCODED_BITS: usize = (16 + 8 * MAX_PSDU_SIZE + 6) * 2 + 288;

pub fn fft_tag_propagation(inputs: &mut [StreamInput], outputs: &mut [StreamOutput]) {
    debug_assert_eq!(inputs[0].consumed().0, outputs[0].produced());
    let (n, tags) = inputs[0].consumed();
    for t in tags.iter().filter(|x| x.index < n) {
        outputs[0].add_tag_abs(t.index, t.tag.clone());
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Modulation {
    Bpsk,
    Qpsk,
    Qam16,
    Qam64,
}

impl Modulation {
    /// bits per symbol
    pub fn n_bpsc(&self) -> usize {
        match self {
            Modulation::Bpsk => 1,
            Modulation::Qpsk => 2,
            Modulation::Qam16 => 4,
            Modulation::Qam64 => 6,
        }
    }
    pub fn map(&self, i: u8) -> Complex32 {
        match self {
            Modulation::Bpsk => {
                const BPSK: [Complex32; 2] = [Complex32::new(-1.0, 0.0), Complex32::new(1.0, 0.0)];
                BPSK[i as usize]
            }
            Modulation::Qpsk => {
                const LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;
                const QPSK: [Complex32; 4] = [
                    Complex32::new(-LEVEL, -LEVEL),
                    Complex32::new(LEVEL, -LEVEL),
                    Complex32::new(-LEVEL, LEVEL),
                    Complex32::new(LEVEL, LEVEL),
                ];
                QPSK[i as usize]
            }
            Modulation::Qam16 => {
                const LEVEL: f32 = 0.31622776601683794;
                const QAM16: [Complex32; 16] = [
                    Complex32::new(-3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 1.0 * LEVEL),
                ];
                QAM16[i as usize]
            }
            Modulation::Qam64 => {
                const LEVEL: f32 = 0.1543033499620919;
                const QAM64: [Complex32; 64] = [
                    Complex32::new(-7.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 3.0 * LEVEL),
                ];
                QAM64[i as usize]
            }
        }
    }

    pub fn demap(&self, i: &Complex32) -> u8 {
        match self {
            Modulation::Bpsk => (i.re > 0.0) as u8,
            Modulation::Qpsk => 2 * (i.im > 0.0) as u8 + (i.re > 0.0) as u8,
            Modulation::Qam16 => {
                let mut ret = 0u8;
                const LEVEL: f32 = 0.6324555320336759;
                let re = i.re;
                let im = i.im;

                ret |= u8::from(re > 0.0);
                ret |= if re.abs() < LEVEL { 2 } else { 0 };
                ret |= if im > 0.0 { 4 } else { 0 };
                ret |= if im.abs() < LEVEL { 8 } else { 0 };
                ret
            }
            Modulation::Qam64 => {
                const LEVEL: f32 = 0.1543033499620919;

                let mut ret = 0;
                let re = i.re;
                let im = i.im;

                ret |= u8::from(re > 0.0);
                ret |= if re.abs() < (4.0 * LEVEL) { 2 } else { 0 };
                ret |= if (re.abs() < (6.0 * LEVEL)) && (re.abs() > (2.0 * LEVEL)) {
                    4
                } else {
                    0
                };
                ret |= if im > 0.0 { 8 } else { 0 };
                ret |= if im.abs() < (4.0 * LEVEL) { 16 } else { 0 };
                ret |= if (im.abs() < (6.0 * LEVEL)) && (im.abs() > (2.0 * LEVEL)) {
                    32
                } else {
                    0
                };

                ret
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
pub enum Mcs {
    Bpsk_1_2,
    Bpsk_3_4,
    Qpsk_1_2,
    Qpsk_3_4,
    Qam16_1_2,
    Qam16_3_4,
    Qam64_2_3,
    Qam64_3_4,
}

impl Mcs {
    pub fn depuncture_pattern(&self) -> &'static [usize] {
        match self {
            Mcs::Bpsk_1_2 | Mcs::Qpsk_1_2 | Mcs::Qam16_1_2 => &[1, 1],
            Mcs::Bpsk_3_4 | Mcs::Qpsk_3_4 | Mcs::Qam16_3_4 | Mcs::Qam64_3_4 => &[1, 1, 1, 0, 0, 1],
            Mcs::Qam64_2_3 => &[1, 1, 1, 0],
        }
    }

    pub fn modulation(&self) -> Modulation {
        match self {
            Mcs::Bpsk_1_2 => Modulation::Bpsk,
            Mcs::Bpsk_3_4 => Modulation::Bpsk,
            Mcs::Qpsk_1_2 => Modulation::Qpsk,
            Mcs::Qpsk_3_4 => Modulation::Qpsk,
            Mcs::Qam16_1_2 => Modulation::Qam16,
            Mcs::Qam16_3_4 => Modulation::Qam16,
            Mcs::Qam64_2_3 => Modulation::Qam64,
            Mcs::Qam64_3_4 => Modulation::Qam64,
        }
    }

    // coded bits per symbol
    pub fn n_cbps(&self) -> usize {
        match self {
            Mcs::Bpsk_1_2 => 48,
            Mcs::Bpsk_3_4 => 48,
            Mcs::Qpsk_1_2 => 96,
            Mcs::Qpsk_3_4 => 96,
            Mcs::Qam16_1_2 => 192,
            Mcs::Qam16_3_4 => 192,
            Mcs::Qam64_2_3 => 288,
            Mcs::Qam64_3_4 => 288,
        }
    }

    // data bits per symbol
    pub fn n_dbps(&self) -> usize {
        match self {
            Mcs::Bpsk_1_2 => 24,
            Mcs::Bpsk_3_4 => 36,
            Mcs::Qpsk_1_2 => 48,
            Mcs::Qpsk_3_4 => 72,
            Mcs::Qam16_1_2 => 96,
            Mcs::Qam16_3_4 => 144,
            Mcs::Qam64_2_3 => 192,
            Mcs::Qam64_3_4 => 216,
        }
    }
    // rate field for signal field
    pub fn rate_field(&self) -> u8 {
        match self {
            Mcs::Bpsk_1_2 => 0x0d,
            Mcs::Bpsk_3_4 => 0x0f,
            Mcs::Qpsk_1_2 => 0x05,
            Mcs::Qpsk_3_4 => 0x07,
            Mcs::Qam16_1_2 => 0x09,
            Mcs::Qam16_3_4 => 0x0b,
            Mcs::Qam64_2_3 => 0x01,
            Mcs::Qam64_3_4 => 0x03,
        }
    }

    pub fn parse(s: &str) -> Result<Mcs, String> {
        let mut m = s.to_string().replace(['-', '_'], "");
        m.make_ascii_lowercase();
        match m.as_str() {
            "bpsk12" => Ok(Mcs::Bpsk_1_2),
            "bpsk34" => Ok(Mcs::Bpsk_3_4),
            "qpsk12" => Ok(Mcs::Qpsk_1_2),
            "qpsk34" => Ok(Mcs::Qpsk_3_4),
            "qam1612" => Ok(Mcs::Qam16_1_2),
            "qam1634" => Ok(Mcs::Qam16_3_4),
            "qam6423" => Ok(Mcs::Qam64_2_3),
            "qam6434" => Ok(Mcs::Qam64_3_4),
            _ => Err(format!("Invalid MCS {}", s)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FrameParam {
    mcs: Mcs,
    psdu_size: usize,
    n_data_bits: usize,
    n_symbols: usize,
    n_pad: usize,
}

impl FrameParam {
    pub fn new(mcs: Mcs, psdu_size: usize) -> Self {
        // n_symbols
        let bits = 16 + 8 * psdu_size + 6;
        let mut n_symbols = bits / mcs.n_dbps();
        if bits % mcs.n_dbps() > 0 {
            n_symbols += 1;
        }

        // n_pad
        let n_data_bits = n_symbols * mcs.n_dbps();
        let n_pad = n_data_bits - (16 + 8 * psdu_size + 6);

        FrameParam {
            mcs,
            psdu_size,
            n_data_bits,
            n_symbols,
            n_pad,
        }
    }
    pub fn psdu_size(&self) -> usize {
        self.psdu_size
    }

    pub fn mcs(&self) -> Mcs {
        self.mcs
    }

    pub fn n_data_bits(&self) -> usize {
        self.n_data_bits
    }

    pub fn n_pad(&self) -> usize {
        self.n_pad
    }

    pub fn n_symbols(&self) -> usize {
        self.n_symbols
    }
}

pub const POLARITY: [Complex32; 127] = [
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
];

pub const LONG: [Complex32; 64] = [
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(-1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(1.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
    Complex32::new(0.0, 0.0),
];
