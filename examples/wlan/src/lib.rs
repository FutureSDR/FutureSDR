use futuresdr::{num_complex::Complex32, runtime::tag::TagAny};

mod decoder;
pub use decoder::Decoder;

mod delay;
pub use delay::Delay;

mod frame_equalizer;
pub use frame_equalizer::FrameEqualizer;

mod fft_shift;
pub use fft_shift::FftShift;

mod moving_average;
pub use moving_average::MovingAverage;

mod sync_long;
pub use sync_long::SyncLong;

mod sync_short;
pub use sync_short::SyncShort;

#[derive(Clone, Copy, Debug)]
pub enum Modulation {
    Bpsk,
    Qpsk,
    Qam16,
    Qam64,
}

impl Modulation {
    /// bits per symbol
    pub fn bps(&self) -> usize {
        match self {
            Modulation::Bpsk => 1,
            Modulation::Qpsk => 2,
            Modulation::Qam16 => 4,
            Modulation::Qam64 => 6,
        }
    }

    pub fn demap(&self, i: &Complex32) -> u8 {
        match self {
            Modulation::Bpsk => (i.re > 0.0) as u8,
            Modulation::Qpsk => 2 * (i.im > 0.0) as u8 + (i.re > 0.0) as u8,
            Modulation::Qam16 => todo!(),
            Modulation::Qam64 => todo!(),
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
    pub fn cbps(&self) -> usize {
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
    pub fn dbps(&self) -> usize {
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
}

#[derive(Clone, Debug)]
pub struct FrameParam {
    mcs: Mcs,
    bytes: usize,
}

impl TagAny for FrameParam {}

impl FrameParam {

    pub fn psdu_size(&self) -> usize {
        self.bytes
    }

    pub fn mcs(&self) -> Mcs {
        self.mcs
    }

    pub fn modulation(&self) -> Modulation {
        self.mcs.modulation()
    }

    pub fn n_data_bits(&self) -> usize {
        self.n_symbols() * self.mcs().dbps()
    }

    pub fn n_symbols(&self) -> usize {
        let bits = 16 + 8 * self.bytes + 6;

        let mut syms = bits / self.mcs.dbps();
        if bits % self.mcs.dbps() > 0 {
            syms += 1;
        }

        syms
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
