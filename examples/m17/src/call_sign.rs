use std::fmt;

#[derive(Clone, Debug)]
pub enum CallSign {
    Broadcast,
    UnitId([u8; 6]),
    Invalid,
    Reserved([u8; 6]),
}

impl CallSign {
    const CHAR_MAP: [u8; 40] = *b" ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-/.";

    pub fn from_bytes(b: &[u8; 6]) -> Self {
        let mut bytes = [0u8; 8];
        bytes[2..8].copy_from_slice(b);
        let encoded = u64::from_be_bytes(bytes);

        match encoded {
            0 => Self::Invalid,
            0xFFFFFFFFFFFF => Self::Broadcast,
            1..=0xEE6B27FFFFFF => Self::UnitId(*b),
            _ => Self::Reserved(*b),
        }
    }

    pub fn new_broadcast() -> Self {
        Self::Broadcast
    }

    pub fn new_id(call: &str) -> Self {
        let call = call.to_string();
        let call = call.to_uppercase();
        let call = call.into_bytes();
        assert!(call.len() <= 9);

        let mut sum = 0u64;

        for c in call.iter().rev() {
            for j in 0..40 {
                if *c == Self::CHAR_MAP[j] {
                    sum = sum * 40 + j as u64;
                    break;
                }
            }
        }

        assert!(sum <= 0xFFFFFFFFFFFF);
        let bytes = sum.to_be_bytes();
        Self::UnitId(bytes[2..8].to_owned().try_into().unwrap())
    }

    pub fn encode(&self) -> &[u8; 6] {
        match self {
            Self::Invalid => &[0; 6],
            Self::Broadcast => &[0xFF; 6],
            Self::UnitId(d) => d,
            Self::Reserved(d) => d,
        }
    }
}

impl fmt::Display for CallSign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Self::Invalid => "#INVLD".to_string(),
            Self::Broadcast => "#BCAST".to_string(),
            Self::UnitId(d) => {
                let mut bytes = [0u8; 8];
                bytes[2..8].copy_from_slice(d);

                let mut encoded = u64::from_be_bytes(bytes);

                let mut out = String::new();
                while encoded > 0 {
                    out.push(Self::CHAR_MAP[(encoded % 40) as usize].try_into().unwrap());
                    encoded /= 40;
                }
                out
            }
            Self::Reserved(_) => "#RSRVD".to_string(),
        };

        write!(f, "{}", s)
    }
}
