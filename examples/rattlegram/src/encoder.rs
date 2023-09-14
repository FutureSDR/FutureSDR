use futuresdr::num_complex::Complex32;
use rustfft::Fft;
use rustfft::FftPlanner;
use std::sync::Arc;

use crate::get_be_bit;
use crate::set_be_bit;
use crate::util::BASE37_BITMAP;
use crate::util::FROZEN_2048_1056;
use crate::util::FROZEN_2048_1392;
use crate::util::FROZEN_2048_712;
use crate::Bch;
use crate::Mls;
use crate::PolarEncoder;
use crate::Psk;
use crate::Xorshift32;

#[derive(Clone, Copy)]
enum OperationMode {
    Null,
    Mode14,
    Mode15,
    Mode16,
}

impl From<OperationMode> for u64 {
    fn from(mode: OperationMode) -> Self {
        match mode {
            OperationMode::Null => 0,
            OperationMode::Mode14 => 14,
            OperationMode::Mode15 => 15,
            OperationMode::Mode16 => 16,
        }
    }
}

pub struct Encoder {
    temp: [Complex32; Self::EXTENDED_LENGTH],
    freq: [Complex32; Self::SYMBOL_LENGTH],
    prev: [Complex32; Self::PAY_CAR_CNT],
    noise_seq: Mls,
    symbol_number: usize,
    code: [i8; Self::CODE_LEN],
    carrier_offset: usize,
    fft_scratch: [Complex32; Self::SYMBOL_LENGTH],
    fft: Arc<dyn Fft<f32>>,
    fancy_line: usize,
    meta_data: u64,
    bch: Bch,
    call: [u8; 9],
    count_down: i64,
    noise_count: u64,
    guard: [Complex32; Self::GUARD_LENGTH],
    mesg: [u8; Self::MAX_BITS / 8],
}

impl Encoder {
    pub const RATE: usize = 48000;
    pub const CODE_ORDER: usize = 11;
    pub const MOD_BITS: usize = 2;
    pub const CODE_LEN: usize = 1 << Self::CODE_ORDER;
    pub const SYMBOL_COUNT: usize = 4;
    pub const SYMBOL_LENGTH: usize = (1280 * Self::RATE) / 8000;
    pub const GUARD_LENGTH: usize = Self::SYMBOL_LENGTH / 8;
    pub const EXTENDED_LENGTH: usize = Self::SYMBOL_LENGTH + Self::GUARD_LENGTH;
    pub const MAX_BITS: usize = 1360;
    pub const COR_SEQ_LEN: isize = 127;
    pub const COR_SEQ_OFF: isize = 1 - Self::COR_SEQ_LEN;
    pub const COR_SEQ_POLY: u64 = 0b10001001;
    pub const PRE_SEQ_LEN: isize = 255;
    pub const PRE_SEQ_OFF: isize = -Self::PRE_SEQ_LEN / 2;
    pub const PRE_SEQ_POLY: u64 = 0b100101011;
    pub const PAY_CAR_CNT: usize = 256;
    pub const PAY_CAR_OFF: isize = -(Self::PAY_CAR_CNT as isize) / 2;
    pub const FANCY_OFF: isize = -(8 * 9 * 3) / 2;
    pub const NOISE_POLY: u64 = 0b100101010001;
    pub const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::Algorithm {
        width: 16,
        poly: 0x2F15,
        init: 0x0000,
        refin: true,
        refout: true,
        xorout: 0x0000,
        check: 0x0000,
        residue: 0x0000,
    });

    pub fn new() -> Self {
        let mut fft_planner = FftPlanner::new();
        let fft = fft_planner.plan_fft_inverse(Self::SYMBOL_LENGTH);

        let bch = Bch::new(&[
            0b100011101,
            0b101110111,
            0b111110011,
            0b101101001,
            0b110111101,
            0b111100111,
            0b100101011,
            0b111010111,
            0b000010011,
            0b101100101,
            0b110001011,
            0b101100011,
            0b100011011,
            0b100111111,
            0b110001101,
            0b100101101,
            0b101011111,
            0b111111001,
            0b111000011,
            0b100111001,
            0b110101001,
            0b000011111,
            0b110000111,
            0b110110001,
        ]);

        Self {
            temp: [Complex32::new(0.0, 0.0); Self::EXTENDED_LENGTH],
            freq: [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH],
            prev: [Complex32::new(0.0, 0.0); Self::PAY_CAR_CNT],
            noise_seq: Mls::new(Self::NOISE_POLY),
            symbol_number: 0,
            code: [0; Self::CODE_LEN],
            carrier_offset: 0,
            fft_scratch: [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH],
            fft,
            fancy_line: 0,
            meta_data: 0,
            bch,
            call: [0; 9],
            count_down: 0,
            noise_count: 0,
            guard: [Complex32::new(0.0, 0.0); Self::GUARD_LENGTH],
            mesg: [0; Self::MAX_BITS / 8],
        }
    }

    pub fn encode(
        &mut self,
        payload: &[u8],
        call_sign: &[u8],
        carrier_frequency: usize,
        noise_symbols: u64,
        fancy_header: bool,
    ) -> Vec<f32> {
        let operation_mode = match payload.len() {
            0 => OperationMode::Null,
            1..=85 => OperationMode::Mode16,
            86..=128 => OperationMode::Mode15,
            _ => OperationMode::Mode14,
        };

        self.carrier_offset = (carrier_frequency * Self::SYMBOL_LENGTH) / Self::RATE;
        let mode: u64 = operation_mode.into();
        self.meta_data = (Self::base37(call_sign) << 8) | mode;

        self.call.fill(0);
        for i in 0..call_sign.len() {
            self.call[i] = Self::base37_map(call_sign[i]);
        }

        self.symbol_number = 0;
        self.count_down = 5;
        self.fancy_line = match fancy_header {
            true => 11,
            false => 0,
        };
        self.noise_count = noise_symbols;

        self.guard.fill(Complex32::new(0.0, 0.0));

        let (data_bits, frozen_bits) = match operation_mode {
            OperationMode::Null => return Vec::new(),
            OperationMode::Mode14 => (1360, FROZEN_2048_1392),
            OperationMode::Mode15 => (1024, FROZEN_2048_1056),
            OperationMode::Mode16 => (680, FROZEN_2048_712),
        };

        let mut scrambler = Xorshift32::new();
        for i in 0..data_bits / 8 {
            let d = if i < payload.len() { payload[i] } else { 0 };
            self.mesg[i] = d ^ scrambler.next() as u8;
        }

        PolarEncoder::encode(
            self.code.as_mut_slice(),
            self.mesg.as_slice(),
            frozen_bits.as_slice(),
            data_bits,
        );

        // ==============================================================
        // CONFIG END
        // ==============================================================
        let mut output = Vec::new();

        loop {
            let mut data_symbol = false;

            match self.count_down {
                5 => {
                    if self.noise_count > 0 {
                        self.noise_count -= 1;
                        self.noise_symbol()
                    } else {
                        self.count_down -= 1;
                        self.schmidl_cox();
                        data_symbol = true;
                        self.count_down -= 1;
                    }
                }
                4 => {
                    self.schmidl_cox();
                    data_symbol = true;
                    self.count_down -= 1;
                }
                3 => {
                    self.preamble();
                    data_symbol = true;
                    self.count_down -= 1;
                    if <OperationMode as Into<u64>>::into(operation_mode) == 0 {
                        self.count_down -= 1;
                    }
                }
                2 => {
                    self.payload_symbol();
                    data_symbol = true;
                    self.symbol_number += 1;
                    if self.symbol_number == Self::SYMBOL_COUNT {
                        self.count_down -= 1;
                    }
                }
                1 => {
                    if self.fancy_line > 0 {
                        self.fancy_line -= 1;
                        self.fancy_symbol();
                    } else {
                        self.silence();
                        self.count_down -= 1;
                    }
                }
                _ => {
                    for _ in 0..Self::EXTENDED_LENGTH {
                        output.push(0.0);
                    }
                    break;
                }
            }

            fn lerp(a: Complex32, b: Complex32, x: f32) -> Complex32 {
                (1.0 - x) * a + x * b
            }

            for i in 0..Self::GUARD_LENGTH {
                let mut x = i as f32 / (Self::GUARD_LENGTH - 1) as f32;
                let ratio = 0.5f32;
                if data_symbol {
                    x = match x.total_cmp(&ratio) {
                        std::cmp::Ordering::Less => x / ratio,
                        _ => 1.0,
                    }
                }
                let y = 0.5 * (1.0 - (std::f32::consts::PI * x).cos());
                let sum = lerp(
                    self.guard[i],
                    self.temp[i + Self::SYMBOL_LENGTH - Self::GUARD_LENGTH],
                    y,
                );
                output.push(sum.re);
            }
            for i in 0..Self::GUARD_LENGTH {
                self.guard[i] = self.temp[i];
            }
            for i in 0..Self::SYMBOL_LENGTH {
                output.push(self.temp[i].re)
            }
        }

        output
    }

    pub fn rate() -> usize {
        Self::RATE
    }

    fn nrz(bit: bool) -> f32 {
        if bit {
            -1.0
        } else {
            1.0
        }
    }

    fn bin(&self, carrier: isize) -> usize {
        (carrier + self.carrier_offset as isize + Self::SYMBOL_LENGTH as isize) as usize
            % Self::SYMBOL_LENGTH
    }

    fn mod_map(b: &[i8; Self::MOD_BITS]) -> Complex32 {
        Psk::<4>::map(b)
    }

    fn base37_map(c: u8) -> u8 {
        if c >= b'0' && c <= b'9' {
            return c - b'0' + 1;
        }
        if c >= b'a' && c <= b'z' {
            return c - b'a' + 11;
        }
        if c >= b'A' && c <= b'Z' {
            return c - b'A' + 11;
        }
        0
    }

    fn base37(str: &[u8]) -> u64 {
        let mut acc = 0u64;
        for c in str {
            acc = 37 * acc + Self::base37_map(*c) as u64;
        }
        acc
    }

    fn noise_symbol(&mut self) {
        let factor = (Self::SYMBOL_LENGTH as f32 / Self::PAY_CAR_CNT as f32).sqrt();
        self.freq.fill(Complex32::new(0.0, 0.0));
        for i in 0..Self::PAY_CAR_CNT {
            self.freq[self.bin(i as isize + Self::PAY_CAR_OFF)] = factor
                * Complex32::new(
                    Self::nrz(self.noise_seq.next()),
                    Self::nrz(self.noise_seq.next()),
                );
        }
        self.transform(false);
    }

    fn payload_symbol(&mut self) {
        self.freq.fill(Complex32::new(0.0, 0.0));

        for i in 0..Self::PAY_CAR_CNT {
            let index = Self::MOD_BITS * (Self::PAY_CAR_CNT * self.symbol_number + i);
            self.prev[i] *= Self::mod_map(&self.code[index..index + 2].try_into().unwrap());
            self.freq[self.bin(i as isize + Self::PAY_CAR_OFF)] = self.prev[i];
        }

        self.transform(true);
    }

    fn silence(&mut self) {
        self.temp.fill(Complex32::new(0.0, 0.0));
    }

    fn transform(&mut self, _papr_reduction: bool) {
        // TODO
        // if papr_reduction && RATE <= 16000 {
        // 	improve_papr(freq);
        //         }
        self.fft.process_outofplace_with_scratch(
            self.freq.as_mut_slice(),
            &mut self.temp[0..Self::SYMBOL_LENGTH],
            self.fft_scratch.as_mut_slice(),
        );
        for i in 0..Self::SYMBOL_LENGTH {
            self.temp[i] /= ((8 * Self::SYMBOL_LENGTH) as f32).sqrt();
        }
    }

    fn schmidl_cox(&mut self) {
        let mut seq = Mls::new(Self::COR_SEQ_POLY);
        let factor = (2 * Self::SYMBOL_LENGTH) as f32 / Self::COR_SEQ_LEN as f32;
        let factor = factor.sqrt();

        self.freq.fill(Complex32::new(0.0, 0.0));
        self.freq[self.bin(Self::COR_SEQ_OFF - 2)] = Complex32::new(factor, 0.0);

        for i in 0..Self::COR_SEQ_LEN {
            self.freq[self.bin(2 * i + Self::COR_SEQ_OFF)] =
                Complex32::new(Self::nrz(seq.next()), 0.0);
        }

        for i in 0..Self::COR_SEQ_LEN {
            self.freq[self.bin(2 * i + Self::COR_SEQ_OFF)] *=
                self.freq[self.bin(2 * (i - 1) + Self::COR_SEQ_OFF)];
        }
        self.transform(false);
    }

    fn fancy_symbol(&mut self) {
        let mut active_carriers = 1;

        for j in 0..9 {
            for i in 0..8 {
                active_carriers +=
                    (BASE37_BITMAP[self.call[j] as usize + 37 * self.fancy_line] >> i) & 1;
            }
        }

        let factor = Self::SYMBOL_LENGTH as f32 / active_carriers as f32;
        let factor = factor.sqrt();

        self.freq.fill(Complex32::new(0.0, 0.0));

        for j in 0..9isize {
            for i in 0..8isize {
                if (BASE37_BITMAP[self.call[j as usize] as usize + 37 * self.fancy_line]
                    & (1 << (7 - i)))
                    != 0
                {
                    self.freq[self.bin((8 * j + i) * 3 + Self::FANCY_OFF)] =
                        Complex32::new(factor * Self::nrz(self.noise_seq.next()), 0.0);
                }
            }
        }
        self.transform(false);
    }

    fn preamble(&mut self) {
        let mut data = [0u8; 9];
        let mut parity = [0u8; 23];

        for i in 0..55 {
            set_be_bit(data.as_mut_slice(), i, ((self.meta_data >> i) & 1) == 1);
        }

        let cs = Self::CRC.checksum(&(self.meta_data << 9).to_le_bytes());

        for i in 0..16 {
            set_be_bit(data.as_mut_slice(), i + 55, ((cs >> i) & 1) == 1);
        }

        self.bch.process(&data, &mut parity);

        let mut seq = Mls::new(Self::PRE_SEQ_POLY);
        let factor = Self::SYMBOL_LENGTH as f32 / Self::PRE_SEQ_LEN as f32;
        let factor = factor.sqrt();
        self.freq.fill(Complex32::new(0.0, 0.0));

        self.freq[self.bin(Self::PRE_SEQ_OFF - 1)] = Complex32::new(factor, 0.0);

        for i in 0..71 {
            self.freq[self.bin(i + Self::PRE_SEQ_OFF)] =
                Self::nrz(get_be_bit(data.as_slice(), i as usize)).into();
        }

        for i in 71..Self::PRE_SEQ_LEN {
            self.freq[self.bin(i + Self::PRE_SEQ_OFF)] =
                Self::nrz(get_be_bit(parity.as_slice(), (i - 71) as usize)).into();
        }

        for i in 0..Self::PRE_SEQ_LEN {
            self.freq[self.bin(i + Self::PRE_SEQ_OFF)] *=
                self.freq[self.bin(i - 1 + Self::PRE_SEQ_OFF)];
        }

        for i in 0..Self::PRE_SEQ_LEN {
            self.freq[self.bin(i + Self::PRE_SEQ_OFF)] *= Self::nrz(seq.next());
        }

        for i in 0..Self::PAY_CAR_CNT {
            self.prev[i] = self.freq[self.bin(i as isize + Self::PAY_CAR_OFF)];
        }

        self.transform(true);
    }
}
