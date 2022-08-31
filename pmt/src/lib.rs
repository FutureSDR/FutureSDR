use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt;

pub trait PmtAny: Any + DynClone + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
dyn_clone::clone_trait_object!(PmtAny);

impl<T: Any + DynClone + Send + Sync + 'static> PmtAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl fmt::Debug for Box<dyn PmtAny> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Box<dyn Any>")
    }
}

impl dyn PmtAny {
    pub fn downcast_ref<T: PmtAny>(&self) -> Option<&T> {
        (*self).as_any().downcast_ref::<T>()
    }
    pub fn downcast_mut<T: PmtAny>(&mut self) -> Option<&mut T> {
        (*self).as_any_mut().downcast_mut::<T>()
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pmt {
    Null,
    String(String),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    VecF32(Vec<f32>),
    VecU64(Vec<u64>),
    Blob(Vec<u8>),
    #[serde(skip)]
    Any(Box<dyn PmtAny>),
}

impl PartialEq for Pmt {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Pmt::Null, Pmt::Null) => true,
            (Pmt::String(x), Pmt::String(y)) => x == y,
            (Pmt::U32(x), Pmt::U32(y)) => x == y,
            (Pmt::U64(x), Pmt::U64(y)) => x == y,
            (Pmt::F32(x), Pmt::F32(y)) => x == y,
            (Pmt::F64(x), Pmt::F64(y)) => x == y,
            (Pmt::VecF32(x), Pmt::VecF32(y)) => x == y,
            (Pmt::VecU64(x), Pmt::VecU64(y)) => x == y,
            (Pmt::Blob(x), Pmt::Blob(y)) => x == y,
            _ => false,
        }
    }
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
            PmtKind::F32 => {
                if let Ok(v) = s.parse::<f32>() {
                    Some(Pmt::F32(v))
                } else {
                    None
                }
            }
            PmtKind::F64 => {
                if let Ok(v) = s.parse::<f64>() {
                    Some(Pmt::F64(v))
                } else {
                    None
                }
            }
            PmtKind::String => Some(Pmt::String(s.to_string())),
            _ => None,
        }
    }
}

#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub enum PmtKind {
    Null,
    String,
    U32,
    U64,
    F32,
    F64,
    VecF32,
    VecF64,
    Blob,
    Any,
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

        let f1 = Pmt::F32(0.1);
        let f2 = Pmt::F32(0.1);
        let f3 = Pmt::F32(0.2);
        assert_eq!(f1, f2);
        assert_ne!(f1, f3);
    }
}
