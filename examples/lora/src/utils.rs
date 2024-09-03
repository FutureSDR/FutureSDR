use std::cmp::Eq;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::hash::Hash;
use std::ops::Mul;
use std::sync::Arc;

use rustfft::Fft;

use futuredsp::firdes::remez;
use futuresdr::num_complex::Complex32;

pub type LLR = f64; // Log-Likelihood Ratio type

pub const LEGACY_SF_5_6: bool = false;

pub const MAX_SF: usize = 12;
pub const LDRO_MAX_DURATION_MS: f32 = 16.;
pub const WHITENING_SEQ: [u8; 255] = [
    0xFF, 0xFE, 0xFC, 0xF8, 0xF0, 0xE1, 0xC2, 0x85, 0x0B, 0x17, 0x2F, 0x5E, 0xBC, 0x78, 0xF1, 0xE3,
    0xC6, 0x8D, 0x1A, 0x34, 0x68, 0xD0, 0xA0, 0x40, 0x80, 0x01, 0x02, 0x04, 0x08, 0x11, 0x23, 0x47,
    0x8E, 0x1C, 0x38, 0x71, 0xE2, 0xC4, 0x89, 0x12, 0x25, 0x4B, 0x97, 0x2E, 0x5C, 0xB8, 0x70, 0xE0,
    0xC0, 0x81, 0x03, 0x06, 0x0C, 0x19, 0x32, 0x64, 0xC9, 0x92, 0x24, 0x49, 0x93, 0x26, 0x4D, 0x9B,
    0x37, 0x6E, 0xDC, 0xB9, 0x72, 0xE4, 0xC8, 0x90, 0x20, 0x41, 0x82, 0x05, 0x0A, 0x15, 0x2B, 0x56,
    0xAD, 0x5B, 0xB6, 0x6D, 0xDA, 0xB5, 0x6B, 0xD6, 0xAC, 0x59, 0xB2, 0x65, 0xCB, 0x96, 0x2C, 0x58,
    0xB0, 0x61, 0xC3, 0x87, 0x0F, 0x1F, 0x3E, 0x7D, 0xFB, 0xF6, 0xED, 0xDB, 0xB7, 0x6F, 0xDE, 0xBD,
    0x7A, 0xF5, 0xEB, 0xD7, 0xAE, 0x5D, 0xBA, 0x74, 0xE8, 0xD1, 0xA2, 0x44, 0x88, 0x10, 0x21, 0x43,
    0x86, 0x0D, 0x1B, 0x36, 0x6C, 0xD8, 0xB1, 0x63, 0xC7, 0x8F, 0x1E, 0x3C, 0x79, 0xF3, 0xE7, 0xCE,
    0x9C, 0x39, 0x73, 0xE6, 0xCC, 0x98, 0x31, 0x62, 0xC5, 0x8B, 0x16, 0x2D, 0x5A, 0xB4, 0x69, 0xD2,
    0xA4, 0x48, 0x91, 0x22, 0x45, 0x8A, 0x14, 0x29, 0x52, 0xA5, 0x4A, 0x95, 0x2A, 0x54, 0xA9, 0x53,
    0xA7, 0x4E, 0x9D, 0x3B, 0x77, 0xEE, 0xDD, 0xBB, 0x76, 0xEC, 0xD9, 0xB3, 0x67, 0xCF, 0x9E, 0x3D,
    0x7B, 0xF7, 0xEF, 0xDF, 0xBF, 0x7E, 0xFD, 0xFA, 0xF4, 0xE9, 0xD3, 0xA6, 0x4C, 0x99, 0x33, 0x66,
    0xCD, 0x9A, 0x35, 0x6A, 0xD4, 0xA8, 0x51, 0xA3, 0x46, 0x8C, 0x18, 0x30, 0x60, 0xC1, 0x83, 0x07,
    0x0E, 0x1D, 0x3A, 0x75, 0xEA, 0xD5, 0xAA, 0x55, 0xAB, 0x57, 0xAF, 0x5F, 0xBE, 0x7C, 0xF9, 0xF2,
    0xE5, 0xCA, 0x94, 0x28, 0x50, 0xA1, 0x42, 0x84, 0x09, 0x13, 0x27, 0x4F, 0x9F, 0x3F, 0x7F,
];

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum Channel {
    #[default]
    EU868_1,
    EU868_2,
    EU868_3,
    EU868_4,
    EU868_5,
    EU868_6,
    EU868_7,
    EU868_8,
    EU868_9,
    EU868_Down,
}

impl From<Channel> for u32 {
    fn from(value: Channel) -> Self {
        match value {
            Channel::EU868_1 => 868_100_000,
            Channel::EU868_2 => 868_300_000,
            Channel::EU868_3 => 868_500_000,
            Channel::EU868_4 => 867_100_000,
            Channel::EU868_5 => 867_300_000,
            Channel::EU868_6 => 867_500_000,
            Channel::EU868_7 => 867_700_000,
            Channel::EU868_8 => 867_900_000,
            Channel::EU868_9 => 868_800_000,
            Channel::EU868_Down => 869_525_000,
        }
    }
}

impl From<Channel> for u64 {
    fn from(value: Channel) -> Self {
        Into::<u32>::into(value) as u64
    }
}

impl From<Channel> for usize {
    fn from(value: Channel) -> Self {
        Into::<u32>::into(value) as usize
    }
}

impl From<Channel> for f32 {
    fn from(value: Channel) -> Self {
        Into::<u32>::into(value) as f32
    }
}

impl From<Channel> for f64 {
    fn from(value: Channel) -> Self {
        Into::<u32>::into(value) as f64
    }
}

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum Bandwidth {
    BW62,
    #[default]
    BW125,
    BW250,
    BW500,
}

impl From<Bandwidth> for u32 {
    fn from(value: Bandwidth) -> Self {
        match value {
            Bandwidth::BW62 => 62_500,
            Bandwidth::BW125 => 125_000,
            Bandwidth::BW250 => 250_000,
            Bandwidth::BW500 => 500_000,
        }
    }
}

impl From<Bandwidth> for u64 {
    fn from(value: Bandwidth) -> Self {
        Into::<u32>::into(value) as u64
    }
}

impl From<Bandwidth> for usize {
    fn from(value: Bandwidth) -> Self {
        Into::<u32>::into(value) as usize
    }
}

impl From<Bandwidth> for f32 {
    fn from(value: Bandwidth) -> Self {
        Into::<u32>::into(value) as f32
    }
}

impl From<Bandwidth> for f64 {
    fn from(value: Bandwidth) -> Self {
        Into::<u32>::into(value) as f64
    }
}

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum SpreadingFactor {
    #[default]
    SF5,
    SF6,
    SF7,
    SF8,
    SF9,
    SF10,
    SF11,
    SF12,
}

impl From<SpreadingFactor> for u8 {
    fn from(value: SpreadingFactor) -> Self {
        match value {
            SpreadingFactor::SF5 => 5,
            SpreadingFactor::SF6 => 6,
            SpreadingFactor::SF7 => 7,
            SpreadingFactor::SF8 => 8,
            SpreadingFactor::SF9 => 9,
            SpreadingFactor::SF10 => 10,
            SpreadingFactor::SF11 => 11,
            SpreadingFactor::SF12 => 12,
        }
    }
}

impl From<SpreadingFactor> for u32 {
    fn from(value: SpreadingFactor) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<SpreadingFactor> for u64 {
    fn from(value: SpreadingFactor) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<SpreadingFactor> for usize {
    fn from(value: SpreadingFactor) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<SpreadingFactor> for f32 {
    fn from(value: SpreadingFactor) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<SpreadingFactor> for f64 {
    fn from(value: SpreadingFactor) -> Self {
        Into::<u8>::into(value).into()
    }
}

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum CodeRate {
    #[default]
    CR_4_5,
    CR_4_6,
    CR_4_7,
    CR_4_8,
}

impl From<CodeRate> for u8 {
    fn from(value: CodeRate) -> Self {
        match value {
            CodeRate::CR_4_5 => 1,
            CodeRate::CR_4_6 => 2,
            CodeRate::CR_4_7 => 3,
            CodeRate::CR_4_8 => 4,
        }
    }
}

impl From<CodeRate> for u32 {
    fn from(value: CodeRate) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<CodeRate> for u64 {
    fn from(value: CodeRate) -> Self {
        Into::<u8>::into(value).into()
    }
}

impl From<CodeRate> for usize {
    fn from(value: CodeRate) -> Self {
        Into::<u8>::into(value).into()
    }
}

#[repr(usize)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LdroMode {
    DISABLE = 0,
    ENABLE = 1,
    AUTO = 2,
}
impl From<usize> for LdroMode {
    fn from(orig: usize) -> Self {
        match orig {
            0_usize => LdroMode::DISABLE,
            1_usize => LdroMode::ENABLE,
            2_usize => LdroMode::AUTO,
            _ => panic!("invalid value to ldro_mode"),
        }
    }
}

/**
 *  \brief  Convert an integer into a MSB first vector of bool
 *
 *  \param  integer
 *          The integer to convert
 *  \param  n_bits
 *          The output number of bits
 */
#[inline]
pub fn int2bool(integer: u16, n_bits: usize) -> Vec<bool> {
    let mut vec: Vec<bool> = vec![false; n_bits];
    let mut j = n_bits;
    for i in 0_usize..n_bits {
        j -= 1;
        vec[j] = ((integer >> i) & 1) != 0;
    }
    vec
}
/**
 *  \brief  Convert a MSB first vector of bool to a integer
 *
 *  \param  b
 *          The boolean vector to convert
 */
#[inline]
pub fn bool2int(b: &[bool]) -> u16 {
    assert!(b.len() <= 16);
    b.iter()
        .map(|x| *x as u16)
        .zip((0_usize..b.len()).rev())
        .map(|(bit, order)| bit << order)
        .sum::<u16>()
}

pub fn build_multichannel_polyphase_filter(num_channels: usize, transition_bw: f64) -> Vec<f32> {
    let filter_coefs = remez::low_pass(
        1.,
        num_channels,
        0.5 - transition_bw / 2.,
        0.5 + transition_bw / 2.,
        0.1,
        100.,
        None,
    );
    filter_coefs.iter().map(|&x| x as f32).collect()
}

/**
 *  \brief  Return an modulated upchirp using s_f=bw
 *
 *  \param  chirp
 *          The pointer to the modulated upchirp
 *  \param  id
 *          The number used to modulate the chirp
 * \param   sf
 *          The spreading factor to use
 * \param os_factor
 *          The oversampling factor used to generate the upchirp
 */
#[inline]
pub fn build_upchirp(id: usize, sf: usize, os_factor: usize) -> Vec<Complex32> {
    let n = 1 << sf;
    let n_fold = n * os_factor - id * os_factor;
    (0..(n * os_factor))
        .map(|j| {
            let osf = os_factor as f32;
            let n = n as f32;
            Complex32::new(1.0, 0.0)
                * Complex32::from_polar(
                    1.,
                    if j < n_fold {
                        let j = j as f32;
                        2.0 * PI * (j * j / (2. * n) / osf / osf + (id as f32 / n - 0.5) * j / osf)
                    } else {
                        let j = j as f32;
                        2.0 * PI * (j * j / (2. * n) / osf / osf + (id as f32 / n - 1.5) * j / osf)
                    },
                )
        })
        .collect()
}

#[inline]
pub fn my_modulo(val1: isize, val2: usize) -> usize {
    if val1 >= 0 {
        (val1 as usize) % val2
    } else {
        (val2 as isize + (val1 % val2 as isize)) as usize % val2
    }
}

#[inline]
pub fn my_roundf(number: f32) -> isize {
    if number > 0.0 {
        (number + 0.5) as isize
    } else {
        (number - 0.5).ceil() as isize
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sample_count(
    sf: usize,
    preamble_len: usize,
    implicit_header: bool,
    payload_len: usize,
    has_crc: bool,
    code_rate: usize,
    os_factor: usize,
    pad: usize,
) -> usize {
    let preamble_symbol_count: f32 = preamble_len as f32 + 4.25;
    let header_symbol_count_before_interleaving: usize = if implicit_header { 0 } else { 5 };
    let payload_symbol_count_before_interleaving: usize =
        2 * payload_len + if has_crc { 4 } else { 0 };
    ((preamble_symbol_count
        + 8.
        + ((payload_symbol_count_before_interleaving + header_symbol_count_before_interleaving
            - (sf - 2)) as f32
            / sf as f32)
            .ceil()
            * (4 + code_rate) as f32)
        * ((1 << sf) * os_factor) as f32) as usize
        + pad * 2
}

/**
 *  \brief  Return the reference chirps using s_f=bw
 *
 *  \param  upchirp
 *          The pointer to the reference upchirp
 *  \param  downchirp
 *          The pointer to the reference downchirp
 * \param   sf
 *          The spreading factor to use
 */
#[inline]
pub fn build_ref_chirps(sf: usize, os_factor: usize) -> (Vec<Complex32>, Vec<Complex32>) {
    let upchirp = build_upchirp(0, sf, os_factor);
    let downchirp = volk_32fc_conjugate_32fc(&upchirp);
    (upchirp, downchirp)
}

pub fn get_symbol_val(
    samples: &[Complex32],
    ref_chirp: &[Complex32],
    fft: &Arc<dyn Fft<f32>>,
) -> Option<usize> {
    // Multiply with ideal downchirp
    let dechirped = volk_32fc_x2_multiply_32fc(samples, ref_chirp);
    let mut cx_out: Vec<Complex32> = dechirped;
    // do the FFT
    fft.process(&mut cx_out);
    // Get magnitude
    let fft_mag = volk_32fc_magnitude_squared_32f(&cx_out);
    let sig_en: f64 = fft_mag.iter().map(|x| *x as f64).fold(0., |acc, e| acc + e);
    // Return argmax here
    if sig_en != 0. {
        Some(argmax_f32(&fft_mag))
    } else {
        None
    }
}

pub fn expand_sync_word(sync_word: Vec<usize>) -> Vec<usize> {
    if sync_word.len() == 1 {
        let tmp = sync_word[0];
        vec![((tmp & 0xF0_usize) >> 4) << 3, (tmp & 0x0F_usize) << 3]
    } else {
        sync_word
    }
}

// find most frequency number in vector
#[inline]
pub fn most_frequent<T>(input_slice: &[T]) -> T
where
    T: Eq + Hash + Copy,
{
    input_slice
        .iter()
        .fold(HashMap::<T, usize>::new(), |mut map, val| {
            map.entry(*val)
                .and_modify(|frq| *frq += 1_usize)
                .or_insert(1_usize);
            map
        })
        .iter()
        .max_by(|(_, val_a), (_, val_b)| val_a.cmp(val_b))
        .map(|(k, _)| k)
        .unwrap_or_else(|| panic!("lora::utils::most_frequent was called on empty slice."))
        .to_owned()
}

pub fn argmax_f32<T: AsRef<[f32]>>(input_slice: T) -> usize {
    input_slice
        .as_ref()
        .iter()
        .enumerate()
        .max_by(|(_, value0), (_, value1)| value0.total_cmp(value1))
        .map(|(idx, _)| idx)
        .unwrap_or(0_usize)
}

pub fn argmax_f64<T: AsRef<[f64]>>(input_slice: T) -> usize {
    input_slice
        .as_ref()
        .iter()
        .enumerate()
        .max_by(|(_, value0), (_, value1)| value0.total_cmp(value1))
        .map(|(idx, _)| idx)
        .unwrap_or(0_usize)
}

#[inline(always)]
pub fn volk_32fc_conjugate_32fc(v: &[Complex32]) -> Vec<Complex32> {
    v.iter().map(|v| v.conj()).collect()
}

#[inline(always)]
pub fn volk_32fc_x2_multiply_32fc<T: Copy + Mul<T, Output = T>>(
    input_slice_1: &[T],
    input_slice_2: &[T],
) -> Vec<T> {
    input_slice_1
        .iter()
        .zip(input_slice_2.iter())
        .map(|(x, y)| *x * *y)
        .collect()
}

#[inline(always)]
pub fn volk_32fc_magnitude_squared_32f(input_slice: &[Complex32]) -> Vec<f32> {
    input_slice
        .iter()
        .map(|x| x.re * x.re + x.im * x.im)
        .collect()
}
