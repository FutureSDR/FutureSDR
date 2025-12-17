use std::cmp::Eq;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::ops::Mul;
use std::ops::Rem;
use std::sync::Arc;

use clap::Arg;
use clap::Command;
use clap::Error;
use clap::ValueEnum;
use clap::builder::PossibleValue;
use clap::builder::TypedValueParser;
use clap::error::ErrorKind;
use num_traits::Num;
// use num_traits::Pow;
use rustfft::Fft;
use strum::IntoEnumIterator;
use strum_macros::Display;
use strum_macros::EnumIter;

use futuredsp::firdes::remez;
use futuresdr::num_complex::Complex32;

use crate::utils::SpreadingFactor::SF7;

pub type LLR = f64; // Log-Likelihood Ratio type

pub const PREAMB_COUNT_DEFAULT: usize = 8;

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

#[derive(Debug, Clone, Copy, Default, EnumIter, Eq, PartialEq, PartialOrd, Ord)]
#[allow(non_camel_case_types)]
#[repr(usize)]
pub enum Channel {
    #[default]
    EU868_1 = 0,
    EU868_2,
    EU868_3,
    EU868_4,
    EU868_5,
    EU868_6,
    EU868_7,
    EU868_8,
    EU868_9,
    EU868_Down,
    Custom(u32),
}

impl clap::ValueEnum for Channel {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Channel::EU868_1,
            Channel::EU868_2,
            Channel::EU868_3,
            Channel::EU868_4,
            Channel::EU868_5,
            Channel::EU868_6,
            Channel::EU868_7,
            Channel::EU868_8,
            Channel::EU868_9,
            Channel::EU868_Down,
            Channel::Custom(0),
        ]
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        let input_uppercase = input.to_uppercase();
        Ok(
            if let Ok(center_freq) = input.replace("_", "").parse::<u32>() {
                Self::from(center_freq)
            } else {
                match if ignore_case {
                    input_uppercase.as_str()
                } else {
                    input
                } {
                    "EU868_1" => Channel::EU868_1,
                    "EU868_2" => Channel::EU868_2,
                    "EU868_3" => Channel::EU868_3,
                    "EU868_4" => Channel::EU868_4,
                    "EU868_5" => Channel::EU868_5,
                    "EU868_6" => Channel::EU868_6,
                    "EU868_7" => Channel::EU868_7,
                    "EU868_8" => Channel::EU868_8,
                    "EU868_9" => Channel::EU868_9,
                    "EU868_Down" => Channel::EU868_Down,
                    _ => return Err(format!("invalid variant: {input}")),
                }
            },
        )
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Channel::EU868_1 => Some(PossibleValue::new("EU868_1")),
            Channel::EU868_2 => Some(PossibleValue::new("EU868_2")),
            Channel::EU868_3 => Some(PossibleValue::new("EU868_3")),
            Channel::EU868_4 => Some(PossibleValue::new("EU868_4")),
            Channel::EU868_5 => Some(PossibleValue::new("EU868_5")),
            Channel::EU868_6 => Some(PossibleValue::new("EU868_6")),
            Channel::EU868_7 => Some(PossibleValue::new("EU868_7")),
            Channel::EU868_8 => Some(PossibleValue::new("EU868_8")),
            Channel::EU868_9 => Some(PossibleValue::new("EU868_9")),
            Channel::EU868_Down => Some(PossibleValue::new("EU868_Down")),
            Channel::Custom(_) => {
                Some(PossibleValue::new("[CustomFrequency]").help("integer center frequency in Hz"))
            }
        }
    }
}

impl Display for Channel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_possible_value().unwrap().get_name())
    }
}

#[derive(Clone)]
pub struct ChannelEnumParser;

impl TypedValueParser for ChannelEnumParser {
    type Value = Channel;
    fn parse_ref(
        &self,
        _cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        let ignore_case = arg.map(|a| a.is_ignore_case_set()).unwrap_or(false);
        match Channel::from_str(value.to_str().unwrap(), ignore_case) {
            Err(msg) => Err(clap::error::Error::raw(ErrorKind::InvalidValue, msg)),
            Ok(value) => Ok(value),
        }
    }
}

impl TryFrom<usize> for Channel {
    type Error = ();

    /// convert an index, NOT the center frequency, to the associated index, channel
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Channel::EU868_1),
            2 => Ok(Channel::EU868_2),
            3 => Ok(Channel::EU868_3),
            4 => Ok(Channel::EU868_4),
            5 => Ok(Channel::EU868_5),
            6 => Ok(Channel::EU868_6),
            7 => Ok(Channel::EU868_7),
            0 => Ok(Channel::EU868_8),
            _ => Err(()),
        }
    }
}

impl TryFrom<Channel> for usize {
    type Error = ();

    /// convert a channel to the associated index, NOT to its center frequency
    fn try_from(value: Channel) -> Result<Self, Self::Error> {
        match value {
            Channel::EU868_1 => Ok(1),
            Channel::EU868_2 => Ok(2),
            Channel::EU868_3 => Ok(3),
            Channel::EU868_4 => Ok(4),
            Channel::EU868_5 => Ok(5),
            Channel::EU868_6 => Ok(6),
            Channel::EU868_7 => Ok(7),
            Channel::EU868_8 => Ok(0),
            _ => Err(()),
        }
    }
}

impl From<u32> for Channel {
    fn from(value: u32) -> Self {
        match value {
            868_100_000 => Channel::EU868_1,
            868_300_000 => Channel::EU868_2,
            868_500_000 => Channel::EU868_3,
            867_100_000 => Channel::EU868_4,
            867_300_000 => Channel::EU868_5,
            867_500_000 => Channel::EU868_6,
            867_700_000 => Channel::EU868_7,
            867_900_000 => Channel::EU868_8,
            869_525_000 => Channel::EU868_Down,
            _ => Channel::Custom(value),
        }
    }
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
            Channel::Custom(center_freq) => center_freq,
        }
    }
}

impl From<Channel> for u64 {
    fn from(value: Channel) -> Self {
        Into::<u32>::into(value) as u64
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

impl TryFrom<u32> for Bandwidth {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            62_500 => Ok(Bandwidth::BW62),
            125_000 => Ok(Bandwidth::BW125),
            250_000 => Ok(Bandwidth::BW250),
            500_000 => Ok(Bandwidth::BW500),
            _ => Err(()),
        }
    }
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

#[derive(
    Debug, Clone, clap::ValueEnum, Copy, Default, EnumIter, PartialEq, Eq, PartialOrd, Ord, Display,
)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum SpreadingFactor {
    #[default]
    SF5 = 0,
    SF6,
    SF7,
    SF8,
    SF9,
    SF10,
    SF11,
    SF12,
}

impl TryFrom<u8> for SpreadingFactor {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            5 => Ok(SpreadingFactor::SF5),
            6 => Ok(SpreadingFactor::SF6),
            7 => Ok(SpreadingFactor::SF7),
            8 => Ok(SpreadingFactor::SF8),
            9 => Ok(SpreadingFactor::SF9),
            10 => Ok(SpreadingFactor::SF10),
            11 => Ok(SpreadingFactor::SF11),
            12 => Ok(SpreadingFactor::SF12),
            _ => Err(()),
        }
    }
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

impl SpreadingFactor {
    pub fn variant_count() -> usize {
        Self::iter().count()
    }

    pub fn smallest() -> SpreadingFactor {
        Self::iter().fold(SpreadingFactor::SF5, |acc, e| {
            if Into::<u8>::into(acc) < Into::<u8>::into(e) {
                acc
            } else {
                e
            }
        })
    }

    pub fn samples_per_symbol(self) -> usize {
        1 << Into::<usize>::into(self)
    }
}

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default, Display)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum CodeRate {
    #[default]
    CR_4_5,
    CR_4_6,
    CR_4_7,
    CR_4_8,
}

impl TryFrom<u8> for CodeRate {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(CodeRate::CR_4_5),
            2 => Ok(CodeRate::CR_4_6),
            3 => Ok(CodeRate::CR_4_7),
            4 => Ok(CodeRate::CR_4_8),
            _ => Err(()),
        }
    }
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
pub fn build_upchirp(
    id: usize,
    sf: SpreadingFactor,
    os_factor: usize,
    preamble: bool,
) -> Vec<Complex32> {
    let n = sf.samples_per_symbol();
    let id = if preamble {
        id
    } else {
        my_modulo(id as isize - 1, n)
    };
    let n_fold = (n - id) * os_factor;
    (0..(n * os_factor))
        .map(|j| {
            let osf = os_factor as f32;
            let n = n as f32;
            Complex32::new(1.0, 0.0)
                * Complex32::from_polar(
                    1.,
                    if j < n_fold {
                        let t = j as f32 / osf;
                        2.0 * PI * (t * t / (2. * n) + (id as f32 / n - 0.5) * t)
                    } else {
                        let t = j as f32 / osf;
                        2.0 * PI * (t * t / (2. * n) + (id as f32 / n - 1.5) * t)
                    },
                )
        })
        .collect()
}

#[inline]
pub fn build_upchirp_phase_coherent(
    id: usize,
    sf: usize,
    os_factor: usize,
    upchirp: bool,
    n_samples: Option<usize>,
    offset_id: bool,
) -> Vec<f32> {
    let n = 1 << sf;
    let n_samples = n_samples.unwrap_or(n * os_factor);
    let mut phase_increment = vec![0.0_f32; n_samples];
    let polarity = if upchirp { 1.0 } else { -1.0 };
    for (t, phase_increment_at_t) in phase_increment.iter_mut().enumerate().take(n_samples) {
        let t_ds = t as f64 / (n * os_factor) as f64;
        let tmp = t_ds - 0.5;
        let mut p = (
            tmp
            // tmp - tmp.pow(21)
        ) as f32
            + (if offset_id {
                id as isize - 1
            } else {
                id as isize
            } as f32
                / n as f32);
        if p > 0.5 {
            p -= 1.0;
        } else if p < -0.5 {
            p += 1.0;
        }
        p *= polarity * (1.0 / os_factor as f32) * (2.0 * PI);
        *phase_increment_at_t = p;
    }
    phase_increment
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
pub fn my_mod<T: Num + Rem + Copy>(val1: T, val2: T) -> T {
    ((val1 % val2) + val2) % val2
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
    sf: SpreadingFactor,
    preamble_len: usize,
    implicit_header: bool,
    payload_len: usize,
    has_crc: bool,
    code_rate: CodeRate,
    os_factor: usize,
    pad: usize,
    ldro: bool,
) -> usize {
    let preamble_symbol_count: f32 = preamble_len as f32 + 4.25 + if sf < SF7 { 2.0 } else { 0.0 };
    let header_symbol_count_before_interleaving: usize = if implicit_header { 0 } else { 5 };
    let payload_symbol_count_before_interleaving: usize =
        2 * payload_len + if has_crc { 4 } else { 0 };
    ((preamble_symbol_count
        + 8.  // header symbol count after interleaving, including the first [(Into::<usize>::into(sf) - if LEGACY_SF_5_6 || sf >= SF7 {2} else {0}))] payload symbols
        + ((payload_symbol_count_before_interleaving + header_symbol_count_before_interleaving
            - (Into::<usize>::into(sf) - if sf >= SF7 {2} else {0})) as f32
            / (Into::<usize>::into(sf) - if ldro {2} else {0}) as f32)
            .ceil()  // 22 -> 21.x
            * (4 + Into::<usize>::into(code_rate)) as f32)
        * ((1 << Into::<usize>::into(sf)) * os_factor) as f32) as usize
        + pad * 2
        - os_factor
}

pub fn align_at_detection_threshold(sf: SpreadingFactor) -> usize {
    (2.5 * (1 << Into::<usize>::into(SpreadingFactor::SF12)) as f32
        - 2.5 * (1 << Into::<usize>::into(sf)) as f32) as usize
}

pub fn encode_str_as_payload(payload: &str, pad_to_len: Option<usize>) -> Vec<u8> {
    let payload_bytes = payload.as_bytes();
    if let Some(frame_len) = pad_to_len {
        let payload_len = payload_bytes.len();
        // frame
        let mut payload: Vec<u8> = (0..(frame_len - payload_len))
            .map(|_| rand::random::<u8>())
            .collect();
        payload.extend(payload_bytes);
        payload
    } else {
        payload_bytes.to_vec()
    }
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
pub fn build_ref_chirps(sf: SpreadingFactor, os_factor: usize) -> (Vec<Complex32>, Vec<Complex32>) {
    let upchirp = build_upchirp(0, sf, os_factor, true);
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

pub type DemodulatedSymbolHardDecoding = u16;
pub type DemodulatedSymbolSoftDecoding = [LLR; MAX_SF];

pub trait DemodulatedSymbol:
    Default + Clone + Copy + std::fmt::Debug + Send + Sync + Sized + 'static
{
}

impl DemodulatedSymbol for DemodulatedSymbolHardDecoding {}
impl DemodulatedSymbol for DemodulatedSymbolSoftDecoding {}

pub type DeinterleavedSymbolHardDecoding = u8;
pub type DeinterleavedSymbolSoftDecoding = [LLR; 8];

pub trait DeinterleavedSymbol:
    Default + Clone + Copy + std::fmt::Debug + Send + Sync + Sized + 'static
{
}

impl DeinterleavedSymbol for DeinterleavedSymbolHardDecoding {}
impl DeinterleavedSymbol for DeinterleavedSymbolSoftDecoding {}
