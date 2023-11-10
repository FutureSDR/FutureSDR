use dyn_clone::DynClone;
use num_complex::Complex32;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// PMT Any trait
///
/// This trait has to be implemented by types that should be used with [`Pmt::Any`].
pub trait PmtAny: Any + DynClone + Send + Sync + 'static {
    /// Cast to [`Any`](std::any::Any)
    fn as_any(&self) -> &dyn Any;
    /// Cast to mutable [`Any`](std::any::Any)
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
    /// Try to cast the [`Pmt::Any`] to the given type.
    pub fn downcast_ref<T: PmtAny>(&self) -> Option<&T> {
        (*self).as_any().downcast_ref::<T>()
    }
    /// Try to cast the [`Pmt::Any`] to the given type mutably.
    pub fn downcast_mut<T: PmtAny>(&mut self) -> Option<&mut T> {
        (*self).as_any_mut().downcast_mut::<T>()
    }
}

/// PMT -- Polymorphic Type
///
/// PMTs are used as input and output for the FutureSDR message passing interface. At the moment,
/// the `Any` type is ignored for de-/serialization.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pmt {
    /// Ok.
    Ok,
    /// Invalid value
    ///
    /// Mainly used as the return type in message handlers, when the parameter is outside the
    /// allowed range.
    InvalidValue,
    /// Null
    ///
    /// Used, for example, as the input type, when the message handler is mainly about the return
    /// type.
    Null,
    /// String
    String(String),
    /// Boolean
    Bool(bool),
    /// Usize
    Usize(usize),
    /// U32, 32-bit unsiged integer
    U32(u32),
    /// U64, 64-bit unsigned integer
    U64(u64),
    /// F32, 32-bit float
    F32(f32),
    /// F64, 64-bit float
    F64(f64),
    /// Vector of 32-bit complex floats.
    VecCF32(Vec<Complex32>),
    /// Vector of 32-bit floats.
    VecF32(Vec<f32>),
    /// Vector of 64-bit floats.
    VecU64(Vec<u64>),
    /// Binary data blob
    Blob(Vec<u8>),
    /// Vector of [`Pmts`](Pmt)
    VecPmt(Vec<Pmt>),
    /// Finished
    ///
    /// Runtime message, used to signal the handler that a connected block finished.
    Finished,
    /// Map (String -> Pmt)
    MapStrPmt(HashMap<String, Pmt>),
    /// Any type
    ///
    /// Wrap anything that implements [`Any`](std::any::Any) in a Pmt. Use
    /// `downcast_ref/mut()` to extract.
    #[serde(skip)]
    Any(Box<dyn PmtAny>),
}

impl fmt::Display for Pmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pmt::Ok => write!(f, "Ok"),
            Pmt::InvalidValue => write!(f, "InvalidValue"),
            Pmt::Null => write!(f, "Null"),
            Pmt::String(v) => write!(f, "{}", v),
            Pmt::Bool(v) => write!(f, "{}", v),
            Pmt::Usize(v) => write!(f, "{}", v),
            Pmt::U32(v) => write!(f, "{}", v),
            Pmt::U64(v) => write!(f, "{}", v),
            Pmt::F32(v) => write!(f, "{}", v),
            Pmt::F64(v) => write!(f, "{}", v),
            Pmt::VecCF32(v) => write!(f, "{:?}", v),
            Pmt::VecF32(v) => write!(f, "{:?}", v),
            Pmt::VecU64(v) => write!(f, "{:?}", v),
            Pmt::Blob(v) => write!(f, "{:?}", v),
            Pmt::VecPmt(v) => write!(f, "{:?}", v),
            Pmt::Finished => write!(f, "Finished"),
            Pmt::MapStrPmt(v) => write!(f, "{:?}", v),
            Pmt::Any(v) => write!(f, "{:?}", v),
        }
    }
}

impl PartialEq for Pmt {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Pmt::Ok, Pmt::Ok) => true,
            (Pmt::InvalidValue, Pmt::InvalidValue) => true,
            (Pmt::Null, Pmt::Null) => true,
            (Pmt::String(x), Pmt::String(y)) => x == y,
            (Pmt::Bool(x), Pmt::Bool(y)) => x == y,
            (Pmt::Usize(x), Pmt::Usize(y)) => x == y,
            (Pmt::U32(x), Pmt::U32(y)) => x == y,
            (Pmt::U64(x), Pmt::U64(y)) => x == y,
            (Pmt::F32(x), Pmt::F32(y)) => x == y,
            (Pmt::F64(x), Pmt::F64(y)) => x == y,
            (Pmt::VecF32(x), Pmt::VecF32(y)) => x == y,
            (Pmt::VecU64(x), Pmt::VecU64(y)) => x == y,
            (Pmt::Blob(x), Pmt::Blob(y)) => x == y,
            (Pmt::VecPmt(x), Pmt::VecPmt(y)) => x == y,
            (Pmt::Finished, Pmt::Finished) => true,
            (Pmt::MapStrPmt(x), Pmt::MapStrPmt(y)) => x == y,
            _ => false,
        }
    }
}

impl std::str::FromStr for Pmt {
    type Err = PmtConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Ok" | "ok" => return Ok(Pmt::Ok),
            "Null" | "null" => return Ok(Pmt::Null),
            "true" => return Ok(Pmt::Bool(true)),
            "false" => return Ok(Pmt::Bool(false)),
            "InvalidValue" | "invalidvalue" => return Ok(Pmt::InvalidValue),
            "Finished" | "finished" => return Ok(Pmt::Finished),
            _ => (),
        }

        if let Ok(p) = serde_json::from_str(s) {
            return Ok(p);
        }

        if let Some((a, b)) = s.split_once(':') {
            let s = format!("{{ \"{}\": {}}}", a, b);
            if let Ok(p) = serde_json::from_str(&s) {
                return Ok(p);
            }
        }
        Err(PmtConversionError)
    }
}

impl Pmt {
    /// Create a [`Pmt`] by parsing a string into a specific [`PmtKind`].
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

/// PMT conversion error.
///
/// This error is returned, if conversion to/from PMTs fail.
#[derive(Debug, Clone, Error, PartialEq)]
#[error("PMt conversion error")]
pub struct PmtConversionError;

impl TryInto<f64> for Pmt {
    type Error = PmtConversionError;

    fn try_into(self) -> Result<f64, Self::Error> {
        match self {
            Pmt::F32(f) => Ok(f as f64),
            Pmt::F64(f) => Ok(f),
            Pmt::U32(f) => Ok(f as f64),
            Pmt::U64(f) => Ok(f as f64),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryInto<usize> for Pmt {
    type Error = PmtConversionError;

    fn try_into(self) -> Result<usize, Self::Error> {
        match self {
            Pmt::Usize(f) => Ok(f),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryInto<u64> for Pmt {
    type Error = PmtConversionError;

    fn try_into(self) -> Result<u64, Self::Error> {
        match self {
            Pmt::U32(v) => Ok(v as u64),
            Pmt::U64(v) => Ok(v),
            Pmt::Usize(v) => Ok(v as u64),
            _ => Err(PmtConversionError),
        }
    }
}

/// PMT types that do not wrap values.
///
/// Usefull for bindings to other languages that do not support Rust's broad enum features.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub enum PmtKind {
    /// Ok
    Ok,
    /// Invalid value
    InvalidValue,
    /// Null
    Null,
    /// String
    String,
    /// Bool
    Bool,
    /// Usize
    Usize,
    /// U32
    U32,
    /// U64
    U64,
    /// F32
    F32,
    /// F64
    F64,
    /// VecCF32
    VecCF32,
    /// VecF32
    VecF32,
    /// VecU64
    VecU64,
    /// Blob
    Blob,
    /// Vec Pmt
    VecPmt,
    /// Finished
    Finished,
    /// Map String -> Pmt
    MapStrPmt,
    /// Any
    Any,
}

impl fmt::Display for PmtKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PmtKind::Ok => write!(f, "Ok"),
            PmtKind::InvalidValue => write!(f, "InvalidValue"),
            PmtKind::Null => write!(f, "Null"),
            PmtKind::String => write!(f, "String"),
            PmtKind::Bool => write!(f, "Bool"),
            PmtKind::Usize => write!(f, "Usize"),
            PmtKind::U32 => write!(f, "U32"),
            PmtKind::U64 => write!(f, "U64"),
            PmtKind::F32 => write!(f, "F32"),
            PmtKind::F64 => write!(f, "F64"),
            PmtKind::VecCF32 => write!(f, "VecCF32"),
            PmtKind::VecF32 => write!(f, "VecF32"),
            PmtKind::VecU64 => write!(f, "VecU64"),
            PmtKind::Blob => write!(f, "Blob"),
            PmtKind::VecPmt => write!(f, "VecPmt"),
            PmtKind::Finished => write!(f, "Finished"),
            PmtKind::MapStrPmt => write!(f, "MapStrPmt"),
            PmtKind::Any => write!(f, "Any"),
        }
    }
}

impl std::str::FromStr for PmtKind {
    type Err = PmtConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Ok" => return Ok(PmtKind::Ok),
            "InvalidValue" => return Ok(PmtKind::InvalidValue),
            "Null" => return Ok(PmtKind::Null),
            "String" => return Ok(PmtKind::String),
            "Bool" => return Ok(PmtKind::Bool),
            "Usize" => return Ok(PmtKind::Usize),
            "U32" => return Ok(PmtKind::U32),
            "U64" => return Ok(PmtKind::U64),
            "F32" => return Ok(PmtKind::F32),
            "F64" => return Ok(PmtKind::F64),
            "VecF32" => return Ok(PmtKind::VecF32),
            "VecU64" => return Ok(PmtKind::VecU64),
            "Blob" => return Ok(PmtKind::Blob),
            "VecPmt" => return Ok(PmtKind::VecPmt),
            "Finished" => return Ok(PmtKind::Finished),
            "MapStrPmt" => return Ok(PmtKind::MapStrPmt),
            "Any" => return Ok(PmtKind::Any),
            _ => (),
        }
        Err(PmtConversionError)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pmt() {
        let p = Pmt::Null;
        assert_eq!(p.to_string(), "Null");
        let p = Pmt::String("foo".to_string());
        assert_eq!(p.to_string(), "foo");
    }

    #[test]
    fn pmt_parse_json() {
        let s = "{ \"U32\": 123 }";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::U32(123)));
        let s = "{ \"Bool\": true }";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::Bool(true)));
        let s = "Bool: true";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::Bool(true)));
        let s = "U32: 123";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::U32(123)));
        let s = "F64: 123";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::F64(123.0)));
        let s = "Blob: [1,2,3]";
        assert_eq!(s.parse::<Pmt>(), Ok(Pmt::Blob(vec![1, 2, 3])));
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

    #[test]
    fn vec_pmt() {
        let vpmt = Pmt::VecPmt(vec![Pmt::U32(1), Pmt::U32(2)]);

        if let Pmt::VecPmt(v) = vpmt {
            assert_eq!(v[0], Pmt::U32(1));
            assert_eq!(v[1], Pmt::U32(2));
        } else {
            panic!("Not a Pmt::VecPmt");
        }
    }

    #[test]
    fn map_str_pmt() {
        let u32val = 42;
        let f64val = 6.02214076e23;

        let msp = Pmt::MapStrPmt(HashMap::from([
            ("str".to_owned(), Pmt::String("a string".to_owned())),
            (
                "submap".to_owned(),
                Pmt::MapStrPmt(HashMap::from([
                    ("U32".to_owned(), Pmt::U32(u32val)),
                    ("F64".to_owned(), Pmt::F64(f64val)),
                ])),
            ),
        ]));

        if let Pmt::MapStrPmt(m) = msp {
            if let Some(Pmt::MapStrPmt(sm)) = m.get("submap") {
                assert_eq!(sm.get("U32"), Some(&Pmt::U32(u32val)));
                assert_eq!(sm.get("F64"), Some(&Pmt::F64(f64val)));
            } else {
                panic!("Could not get submap");
            }
        } else {
            panic!("Not a Pmt::MapStrPmt");
        }
    }
}
