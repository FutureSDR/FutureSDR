use base64::prelude::*;
use ctr::cipher::{KeyIvInit, StreamCipher};
use futuresdr::tracing::info;
use meshtastic::Message;

use crate::utils::Bandwidth;
use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;

type Aes128Ctr64LE = ctr::Ctr64LE<aes::Aes128>;
type Aes256Ctr64LE = ctr::Ctr64LE<aes::Aes256>;

const DEFAULT_KEY: [u8; 16] = [
    0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01,
];

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(non_camel_case_types)]
pub enum MeshtasticConfig {
    ShortFast,
    ShortSlow,
    MediumFast,
    MediumSlow,
    #[default]
    LongFast,
    LongModerate,
    LongSlow,
    VeryLongSlow,
}

impl MeshtasticConfig {
    pub fn to_config(&self) -> (Bandwidth, SpreadingFactor, CodeRate, u32, bool) {
        match self {
            Self::ShortFast => (
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                869525000,
                false,
            ),
            Self::ShortSlow => (
                Bandwidth::BW250,
                SpreadingFactor::SF8,
                CodeRate::CR_4_5,
                869525000,
                false,
            ),
            Self::MediumFast => (
                Bandwidth::BW250,
                SpreadingFactor::SF9,
                CodeRate::CR_4_5,
                869525000,
                false,
            ),
            Self::MediumSlow => (
                Bandwidth::BW250,
                SpreadingFactor::SF10,
                CodeRate::CR_4_5,
                869525000,
                false,
            ),
            Self::LongFast => (
                Bandwidth::BW250,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                869525000,
                false,
            ),
            Self::LongModerate => (
                Bandwidth::BW125,
                SpreadingFactor::SF11,
                CodeRate::CR_4_8,
                869587500,
                true,
            ),
            Self::LongSlow => (
                Bandwidth::BW125,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                869587500,
                true,
            ),
            Self::VeryLongSlow => (
                Bandwidth::BW62,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                869492500,
                true,
            ),
        }
    }
}

#[derive(Debug)]
struct MeshPacket {
    _dest: u32,
    sender: u32,
    packet_id: u32,
    _flags: u8,
    channel_hash: u8,
    _reserved: u16,
    data: Vec<u8>,
}

impl MeshPacket {
    fn new(bytes: &[u8]) -> Self {
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

pub struct MeshtasticChannels {
    channels: Vec<(u8, String, Key)>,
}

impl MeshtasticChannels {
    pub fn new() -> Self {
        Self {
            channels: vec![(8, "Primary".to_string(), Key::Aes128(DEFAULT_KEY))],
        }
    }

    pub fn add_channel(&mut self, name: &str, key: &str) {
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

        self.channels.push((hash, name, key));
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

    pub fn decode(&self, bytes: &[u8]) {
        let packet = MeshPacket::new(bytes);

        for chan in self.channels.iter() {
            if packet.channel_hash == chan.0 {
                let mut iv = vec![];
                iv.extend_from_slice(&(packet.packet_id as u64).to_le_bytes());
                iv.extend_from_slice(&(packet.sender as u64).to_le_bytes());
                let iv: [u8; 16] = iv.try_into().unwrap();

                let mut bytes = packet.data.clone();
                match chan.2 {
                    Key::Aes128(key) => {
                        let mut cipher = Aes128Ctr64LE::new(&key.into(), &iv.into());
                        cipher.apply_keystream(&mut bytes);

                        if let Ok(res) = meshtastic::protobufs::Data::decode(&*bytes) {
                            if res.portnum == meshtastic::protobufs::PortNum::TextMessageApp as i32
                            {
                                info!(
                                    "Channel {}: Message {:?}",
                                    chan.1,
                                    String::from_utf8_lossy(&res.payload)
                                );
                            } else {
                                info!("Channel {}: Message {:?}", chan.1, res);
                            }
                            break;
                        }
                    }
                    Key::Aes256(key) => {
                        let mut cipher = Aes256Ctr64LE::new(&key.into(), &iv.into());
                        cipher.apply_keystream(&mut bytes);

                        if let Ok(res) = meshtastic::protobufs::Data::decode(&*bytes) {
                            if res.portnum == meshtastic::protobufs::PortNum::TextMessageApp as i32
                            {
                                info!(
                                    "Channel {}: Message {:?}",
                                    chan.1,
                                    String::from_utf8_lossy(&res.payload)
                                );
                            } else {
                                info!("Channel {}: Message {:?}", chan.1, res);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl Default for MeshtasticChannels {
    fn default() -> Self {
        Self::new()
    }
}
