use crate::utils::Bandwidth;
use crate::utils::Channel;
use crate::utils::CodeRate;
use crate::utils::LdroMode;
use crate::utils::SpreadingFactor;
use base64::prelude::*;
use clap::Arg;
use clap::Command;
use clap::Error;
use clap::ValueEnum;
use clap::builder::PossibleValue;
use clap::builder::TypedValueParser;
use clap::error::ErrorKind;
use ctr::cipher::KeyIvInit;
use ctr::cipher::StreamCipher;
use futuresdr::tracing::info;
use meshtastic::Message;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;

type Aes128 = ctr::Ctr64BE<aes::Aes128>;
type Aes256 = ctr::Ctr64BE<aes::Aes256>;

const DEFAULT_KEY: [u8; 16] = [
    0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01,
];

#[derive(Debug, Clone, Copy, Default)]
#[allow(non_camel_case_types)]
pub enum MeshtasticConfig {
    ShortFastEu,
    ShortSlowEu,
    MediumFastEu,
    MediumSlowEu,
    #[default]
    LongFastEu,
    LongModerateEu,
    LongSlowEu,
    VeryLongSlowEu,
    ShortTurboUs,
    ShortFastUs,
    ShortSlowUs,
    MediumFastUs,
    MediumSlowUs,
    LongTurboUs,
    LongFastUs,
    LongModerateUs,
    LongSlowUs,
    VeryLongSlowUs,
    Custom(Bandwidth, SpreadingFactor, CodeRate, Channel, LdroMode),
}

impl MeshtasticConfig {
    fn parse_custom_components(input: &str, ignore_case: bool) -> Result<Self, String> {
        let components: Vec<_> = input.split(',').map(str::trim).collect();
        if components.len() != 5 {
            return Err(format!(
                "invalid custom Meshtastic config '{input}': expected 5 comma-separated values"
            ));
        }

        let bandwidth = <Bandwidth as ValueEnum>::from_str(components[0], ignore_case)?;
        let spreading_factor =
            <SpreadingFactor as ValueEnum>::from_str(components[1], ignore_case)?;
        let code_rate = <CodeRate as ValueEnum>::from_str(components[2], ignore_case)?;
        let channel = <Channel as ValueEnum>::from_str(components[3], ignore_case)?;
        let ldro = <LdroMode as ValueEnum>::from_str(components[4], ignore_case)?;
        Ok(Self::Custom(
            bandwidth,
            spreading_factor,
            code_rate,
            channel,
            ldro,
        ))
    }

    fn strip_custom_wrapper(input: &str, ignore_case: bool) -> Option<&str> {
        let trimmed = input.trim();
        if let Some(inner) = trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            return Some(inner);
        }

        if ignore_case {
            let prefix = "custom(";
            if trimmed.len() > prefix.len()
                && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix)
                && trimmed.ends_with(')')
            {
                return Some(&trimmed[prefix.len()..trimmed.len() - 1]);
            }
        } else if let Some(inner) = trimmed
            .strip_prefix("Custom(")
            .and_then(|s| s.strip_suffix(')'))
        {
            return Some(inner);
        }

        None
    }

    fn parse_custom(input: &str, ignore_case: bool) -> Option<Result<Self, String>> {
        let trimmed = input.trim();
        if let Some(inner) = Self::strip_custom_wrapper(trimmed, ignore_case) {
            return Some(Self::parse_custom_components(inner, ignore_case));
        }

        if trimmed.contains(',') {
            return Some(Self::parse_custom_components(trimmed, ignore_case));
        }

        None
    }

    pub fn to_config(&self) -> (Bandwidth, SpreadingFactor, CodeRate, Channel, LdroMode) {
        match self {
            Self::ShortFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                LdroMode::DISABLE,
            ),
            Self::ShortSlowEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF8,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                LdroMode::DISABLE,
            ),
            Self::MediumFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF9,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                LdroMode::DISABLE,
            ),
            Self::MediumSlowEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF10,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                LdroMode::DISABLE,
            ),
            Self::LongFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                LdroMode::DISABLE,
            ),
            Self::LongModerateEu => (
                Bandwidth::BW125,
                SpreadingFactor::SF11,
                CodeRate::CR_4_8,
                Channel::Custom(869587500),
                LdroMode::ENABLE,
            ),
            Self::LongSlowEu => (
                Bandwidth::BW125,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(869587500),
                LdroMode::ENABLE,
            ),
            Self::VeryLongSlowEu => (
                Bandwidth::BW62,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(869492500),
                LdroMode::ENABLE,
            ),
            Self::ShortTurboUs => (
                Bandwidth::BW500,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::ShortFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::ShortSlowUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF8,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::MediumFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF9,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::MediumSlowUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF10,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::LongTurboUs => (
                Bandwidth::BW500,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::LongFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                LdroMode::DISABLE,
            ),
            Self::LongModerateUs => (
                Bandwidth::BW125,
                SpreadingFactor::SF11,
                CodeRate::CR_4_8,
                Channel::Custom(904437500),
                LdroMode::ENABLE,
            ),
            Self::LongSlowUs => (
                Bandwidth::BW125,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(904437500),
                LdroMode::ENABLE,
            ),
            Self::VeryLongSlowUs => (
                Bandwidth::BW62,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(916218750),
                LdroMode::ENABLE,
            ),
            Self::Custom(bw, sf, cr, freq, ldro) => (*bw, *sf, *cr, *freq, *ldro),
        }
    }
}

impl clap::ValueEnum for MeshtasticConfig {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::ShortFastEu,
            Self::ShortSlowEu,
            Self::MediumFastEu,
            Self::MediumSlowEu,
            Self::LongFastEu,
            Self::LongModerateEu,
            Self::LongSlowEu,
            Self::VeryLongSlowEu,
            Self::ShortTurboUs,
            Self::ShortFastUs,
            Self::ShortSlowUs,
            Self::MediumFastUs,
            Self::MediumSlowUs,
            Self::LongTurboUs,
            Self::LongFastUs,
            Self::LongModerateUs,
            Self::LongSlowUs,
            Self::VeryLongSlowUs,
            Self::Custom(
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(868595000),
                LdroMode::DISABLE,
            ),
        ]
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        if let Some(custom) = Self::parse_custom(input, ignore_case) {
            return custom;
        }

        let input_uppercase = input.to_uppercase();
        match if ignore_case {
            input_uppercase.as_str()
        } else {
            input
        } {
            "SHORT_FAST_EU" => Ok(Self::ShortFastEu),
            "SHORT_SLOW_EU" => Ok(Self::ShortSlowEu),
            "MEDIUM_FAST_EU" => Ok(Self::MediumFastEu),
            "MEDIUM_SLOW_EU" => Ok(Self::MediumSlowEu),
            "LONG_FAST_EU" => Ok(Self::LongFastEu),
            "LONG_MODERATE_EU" => Ok(Self::LongModerateEu),
            "LONG_SLOW_EU" => Ok(Self::LongSlowEu),
            "VERY_LONG_SLOW_EU" => Ok(Self::VeryLongSlowEu),
            "SHORT_TURBO_US" => Ok(Self::ShortTurboUs),
            "SHORT_FAST_US" => Ok(Self::ShortFastUs),
            "SHORT_SLOW_US" => Ok(Self::ShortSlowUs),
            "MEDIUM_FAST_US" => Ok(Self::MediumFastUs),
            "MEDIUM_SLOW_US" => Ok(Self::MediumSlowUs),
            "LONG_TURBO_US" => Ok(Self::LongTurboUs),
            "LONG_FAST_US" => Ok(Self::LongFastUs),
            "LONG_MODERATE_US" => Ok(Self::LongModerateUs),
            "LONG_SLOW_US" => Ok(Self::LongSlowUs),
            "VERY_LONG_SLOW_US" => Ok(Self::VeryLongSlowUs),
            _ => Err(format!("invalid variant: {input}")),
        }
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::ShortFastEu => Some(PossibleValue::new("SHORT_FAST_EU")),
            Self::ShortSlowEu => Some(PossibleValue::new("SHORT_SLOW_EU")),
            Self::MediumFastEu => Some(PossibleValue::new("MEDIUM_FAST_EU")),
            Self::MediumSlowEu => Some(PossibleValue::new("MEDIUM_SLOW_EU")),
            Self::LongFastEu => Some(PossibleValue::new("LONG_FAST_EU")),
            Self::LongModerateEu => Some(PossibleValue::new("LONG_MODERATE_EU")),
            Self::LongSlowEu => Some(PossibleValue::new("LONG_SLOW_EU")),
            Self::VeryLongSlowEu => Some(PossibleValue::new("VERY_LONG_SLOW_EU")),
            Self::ShortTurboUs => Some(PossibleValue::new("SHORT_TURBO_US")),
            Self::ShortFastUs => Some(PossibleValue::new("SHORT_FAST_US")),
            Self::ShortSlowUs => Some(PossibleValue::new("SHORT_SLOW_US")),
            Self::MediumFastUs => Some(PossibleValue::new("MEDIUM_FAST_US")),
            Self::MediumSlowUs => Some(PossibleValue::new("MEDIUM_SLOW_US")),
            Self::LongTurboUs => Some(PossibleValue::new("LONG_TURBO_US")),
            Self::LongFastUs => Some(PossibleValue::new("LONG_FAST_US")),
            Self::LongModerateUs => Some(PossibleValue::new("LONG_MODERATE_US")),
            Self::LongSlowUs => Some(PossibleValue::new("LONG_SLOW_US")),
            Self::VeryLongSlowUs => Some(PossibleValue::new("VERY_LONG_SLOW_US")),
            Self::Custom(_, _, _, _, _) => Some(
                PossibleValue::new(
                    "Custom([bandwidth], [spreading_factor], [code_rate], [channel], [ldro])",
                )
                .help(
                    "Also accepts '([..])' or bare '[..]' with the same 5 comma-separated values.",
                ),
            ),
        }
    }
}

impl Display for MeshtasticConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShortFastEu => write!(f, "SHORT_FAST_EU"),
            Self::ShortSlowEu => write!(f, "SHORT_SLOW_EU"),
            Self::MediumFastEu => write!(f, "MEDIUM_FAST_EU"),
            Self::MediumSlowEu => write!(f, "MEDIUM_SLOW_EU"),
            Self::LongFastEu => write!(f, "LONG_FAST_EU"),
            Self::LongModerateEu => write!(f, "LONG_MODERATE_EU"),
            Self::LongSlowEu => write!(f, "LONG_SLOW_EU"),
            Self::VeryLongSlowEu => write!(f, "VERY_LONG_SLOW_EU"),
            Self::ShortTurboUs => write!(f, "SHORT_TURBO_US"),
            Self::ShortFastUs => write!(f, "SHORT_FAST_US"),
            Self::ShortSlowUs => write!(f, "SHORT_SLOW_US"),
            Self::MediumFastUs => write!(f, "MEDIUM_FAST_US"),
            Self::MediumSlowUs => write!(f, "MEDIUM_SLOW_US"),
            Self::LongTurboUs => write!(f, "LONG_TURBO_US"),
            Self::LongFastUs => write!(f, "LONG_FAST_US"),
            Self::LongModerateUs => write!(f, "LONG_MODERATE_US"),
            Self::LongSlowUs => write!(f, "LONG_SLOW_US"),
            Self::VeryLongSlowUs => write!(f, "VERY_LONG_SLOW_US"),
            Self::Custom(bandwidth, spreading_factor, code_rate, channel, ldro) => write!(
                f,
                "Custom({bandwidth}, {spreading_factor}, {code_rate}, {channel}, {ldro})"
            ),
        }
    }
}

#[derive(Clone)]
pub struct MeshtasticConfigEnumParser;

impl TypedValueParser for MeshtasticConfigEnumParser {
    type Value = MeshtasticConfig;
    fn parse_ref(
        &self,
        _cmd: &Command,
        arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        let ignore_case = arg.map(|a| a.is_ignore_case_set()).unwrap_or(false);
        match MeshtasticConfig::from_str(value.to_str().unwrap(), ignore_case) {
            Err(msg) => Err(clap::error::Error::raw(ErrorKind::InvalidValue, msg)),
            Ok(value) => Ok(value),
        }
    }
}

#[derive(Debug)]
pub struct MeshPacket {
    _dest: u32,
    sender: u32,
    packet_id: u32,
    _flags: u8,
    channel_hash: u8,
    _reserved: u16,
    data: Vec<u8>,
}

impl MeshPacket {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            _dest: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            sender: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            packet_id: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            _flags: bytes[12],
            channel_hash: bytes[13],
            _reserved: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
            data: bytes[16..].to_vec(),
        }
    }
}

#[derive(Debug)]
enum Key {
    Aes128([u8; 16]),
    Aes256([u8; 32]),
}

impl Key {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Aes128(x) => x,
            Self::Aes256(x) => x,
        }
    }
}

#[derive(Debug)]
pub struct MeshtasticChannel {
    key: Key,
    hash: u8,
    name: String,
}

impl MeshtasticChannel {
    pub fn new(name: &str, key: &str) -> Self {
        let key = BASE64_STANDARD.decode(key).unwrap();
        let key = if key == [0x01] {
            Key::Aes128(DEFAULT_KEY)
        } else if key.len() == 16 {
            Key::Aes128(key.clone().try_into().unwrap())
        } else if key.len() == 32 {
            Key::Aes256(key.clone().try_into().unwrap())
        } else {
            panic!("wrong key (base64-encoded 1/16/32 bytes expected)");
        };

        let (hash, name) = if name.is_empty() || name == "\n" {
            let hash = Self::hash("\n", key.as_slice());
            (hash, "<unset>".to_string())
        } else {
            let hash = Self::hash(name, key.as_slice());
            (hash, name.to_string())
        };

        Self { key, hash, name }
    }

    fn hash(name: &str, key: &[u8]) -> u8 {
        let mut xor = 0;
        for x in name.bytes() {
            xor ^= x;
        }
        for x in key.iter() {
            xor ^= x;
        }
        xor
    }

    pub fn decode(&self, packet: &MeshPacket) -> bool {
        info!("MeshPacket: {:?}", packet);
        let mut iv = vec![];
        iv.extend_from_slice(&(packet.packet_id as u64).to_le_bytes());
        iv.extend_from_slice(&(packet.sender as u64).to_le_bytes());
        let iv: [u8; 16] = iv.try_into().unwrap();

        let mut bytes = packet.data.clone();
        match self.key {
            Key::Aes128(key) => {
                let mut cipher = Aes128::new(&key.into(), &iv.into());
                cipher.apply_keystream(&mut bytes);
            }
            Key::Aes256(key) => {
                let mut cipher = Aes256::new(&key.into(), &iv.into());
                cipher.apply_keystream(&mut bytes);
            }
        }
        if let Ok(res) = meshtastic::protobufs::Data::decode(&*bytes) {
            if res.portnum == meshtastic::protobufs::PortNum::TextMessageApp as i32 {
                info!(
                    "Channel {}: Message {:?}",
                    self.name,
                    String::from_utf8_lossy(&res.payload)
                );
                true
            } else {
                info!("Channel {}: Message {:?}", self.name, res);
                true
            }
        } else {
            false
        }
    }

    pub fn encode(&self, data: String) -> Vec<u8> {
        let packet_id = 0u32;
        let dest = 0xffffffffu32;
        let sender = 0x3a48290eu32;

        let data = meshtastic::protobufs::Data {
            portnum: 1,
            payload: data.into_bytes(),
            want_response: false,
            dest: 0,
            source: 0,
            request_id: 0,
            reply_id: 0,
            emoji: 0,
            bitfield: None,
        };

        let mut bytes = data.encode_to_vec();

        let mut iv = vec![];
        iv.extend_from_slice(&(packet_id as u64).to_le_bytes());
        iv.extend_from_slice(&(sender as u64).to_le_bytes());
        let iv: [u8; 16] = iv.try_into().unwrap();

        match self.key {
            Key::Aes128(key) => {
                let mut cipher = Aes128::new(&key.into(), &iv.into());
                cipher.apply_keystream(&mut bytes);
            }
            Key::Aes256(key) => {
                let mut cipher = Aes256::new(&key.into(), &iv.into());
                cipher.apply_keystream(&mut bytes);
            }
        }

        let mut out = vec![];
        out.extend_from_slice(&dest.to_le_bytes());
        out.extend_from_slice(&sender.to_le_bytes());
        out.extend_from_slice(&packet_id.to_le_bytes());
        out.push(0);
        out.push(self.hash);
        out.extend_from_slice(&[0; 2]);
        out.extend_from_slice(&bytes);
        out
    }
}

pub struct MeshtasticChannels {
    channels: Vec<MeshtasticChannel>,
}

impl MeshtasticChannels {
    pub fn new() -> Self {
        Self {
            channels: vec![MeshtasticChannel::new("", "AQ==")],
        }
    }

    pub fn add_channel(&mut self, chan: MeshtasticChannel) {
        self.channels.push(chan);
    }

    pub fn decode(&self, bytes: &[u8]) {
        let packet = MeshPacket::new(bytes);

        for chan in self.channels.iter() {
            if packet.channel_hash == chan.hash && chan.decode(&packet) {
                return;
            }
        }
        self.channels[0].decode(&packet);
    }
}

impl Default for MeshtasticChannels {
    fn default() -> Self {
        Self::new()
    }
}
