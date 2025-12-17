use base64::prelude::*;
use ctr::cipher::KeyIvInit;
use ctr::cipher::StreamCipher;
use futuresdr::tracing::info;
use meshtastic::Message;

use crate::utils::Bandwidth;
use crate::utils::Channel;
use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;

type Aes128 = ctr::Ctr64BE<aes::Aes128>;
type Aes256 = ctr::Ctr64BE<aes::Aes256>;

const DEFAULT_KEY: [u8; 16] = [
    0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01,
];

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
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
    ShortFastUs,
    ShortSlowUs,
    MediumFastUs,
    MediumSlowUs,
    LongFastUs,
    LongModerateUs,
    LongSlowUs,
    VeryLongSlowUs,
}

impl MeshtasticConfig {
    pub fn to_config(&self) -> (Bandwidth, SpreadingFactor, CodeRate, Channel, bool) {
        match self {
            Self::ShortFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                false,
            ),
            Self::ShortSlowEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF8,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                false,
            ),
            Self::MediumFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF9,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                false,
            ),
            Self::MediumSlowEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF10,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                false,
            ),
            Self::LongFastEu => (
                Bandwidth::BW250,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                Channel::Custom(869525000),
                false,
            ),
            Self::LongModerateEu => (
                Bandwidth::BW125,
                SpreadingFactor::SF11,
                CodeRate::CR_4_8,
                Channel::Custom(869587500),
                true,
            ),
            Self::LongSlowEu => (
                Bandwidth::BW125,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(869587500),
                true,
            ),
            Self::VeryLongSlowEu => (
                Bandwidth::BW62,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(869492500),
                true,
            ),
            Self::ShortFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF7,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                false,
            ),
            Self::ShortSlowUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF8,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                false,
            ),
            Self::MediumFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF9,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                false,
            ),
            Self::MediumSlowUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF10,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                false,
            ),
            Self::LongFastUs => (
                Bandwidth::BW250,
                SpreadingFactor::SF11,
                CodeRate::CR_4_5,
                Channel::Custom(906875000),
                false,
            ),
            Self::LongModerateUs => (
                Bandwidth::BW125,
                SpreadingFactor::SF11,
                CodeRate::CR_4_8,
                Channel::Custom(904437500),
                true,
            ),
            Self::LongSlowUs => (
                Bandwidth::BW125,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(904437500),
                true,
            ),
            Self::VeryLongSlowUs => (
                Bandwidth::BW62,
                SpreadingFactor::SF12,
                CodeRate::CR_4_8,
                Channel::Custom(916218750),
                true,
            ),
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
