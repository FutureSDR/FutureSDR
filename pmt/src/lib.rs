use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Pmt {
    Null,
    String(String),
    U32(u32),
    U64(u64),
    Double(f64),
    VecF32(Vec<f32>),
    VecU64(Vec<u64>),
    Blob(Vec<u8>),
}

impl Pmt {
    pub fn is_string(&self) -> bool {
        matches!(self, Pmt::String(_))
    }

    pub fn to_string(&self) -> Option<String> {
        match &self {
            Pmt::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn from_string(s: &str, t: &PmtKind) -> Option<Pmt> {
        match t {
            PmtKind::U32 => {
                if let Ok(v) = s.parse::<u32>() {
                    Some(Pmt::U32(v))
                } else {
                    None
                }
            }
            PmtKind::U64 => {
                if let Ok(v) = s.parse::<u64>() {
                    Some(Pmt::U64(v))
                } else {
                    None
                }
            }
            PmtKind::Double => {
                if let Ok(v) = s.parse::<f64>() {
                    Some(Pmt::Double(v))
                } else {
                    None
                }
            }
            PmtKind::String => Some(Pmt::String(s.to_string())),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum PmtKind {
    String,
    U32,
    U64,
    Double,
    VecF32,
    Blob,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pmt() {
        let p = Pmt::Null;
        assert!(!p.is_string());
        assert_eq!(p.to_string(), None);
        let p = Pmt::String("foo".to_owned());
        assert!(p.is_string());
        assert_eq!(p.to_string(), Some("foo".to_owned()));
    }

    #[test]
    fn pmt_serde() {
        let p = Pmt::Null;
        let mut s = flexbuffers::FlexbufferSerializer::new();
        p.serialize(&mut s).unwrap();

        let r = flexbuffers::Reader::get_root(s.view()).unwrap();
        let p2 = Pmt::deserialize(r).unwrap();

        assert_eq!(p, p2);
    }

    #[allow(clippy::many_single_char_names)]
    #[test]
    fn pmt_eq() {
        let a = Pmt::Null;
        let b = Pmt::U32(123);
        assert_ne!(a, b);

        let c = Pmt::Null;
        let d = Pmt::U32(12);
        let e = Pmt::U32(123);
        assert_eq!(a, c);
        assert_eq!(b, e);
        assert_ne!(b, d);
    }
}
