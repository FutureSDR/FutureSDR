use dyn_clone::DynClone;
use num_complex::Complex32;
use serde::Deserialize;
use serde::Serialize;
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
    /// Consume `self`, converting to a `Box<dyn Any>`
    fn to_any(self: Box<Self>) -> Box<dyn Any>;
}
dyn_clone::clone_trait_object!(PmtAny);

impl<T: Any + DynClone + Send + Sync + 'static> PmtAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn to_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl fmt::Debug for Box<dyn PmtAny> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Box<dyn Any>")
    }
}

impl dyn PmtAny {
    /// Determine if this `PmtAny` has the given concrete type.
    ///
    /// A value of `true` implies `downcast_ref`, `downcast_mut` and `take` will return `Some`.
    pub fn is<T: PmtAny>(&self) -> bool {
        self.as_any().is::<T>()
    }
    /// Try to cast the [`Pmt::Any`] to the given type.
    pub fn downcast_ref<T: PmtAny>(&self) -> Option<&T> {
        (*self).as_any().downcast_ref::<T>()
    }
    /// Try to cast the [`Pmt::Any`] to the given type mutably.
    pub fn downcast_mut<T: PmtAny>(&mut self) -> Option<&mut T> {
        (*self).as_any_mut().downcast_mut::<T>()
    }
    /// Consuming `self`, try to take ownership of the value as the given type.
    pub fn take<T: PmtAny>(self: Box<Self>) -> Option<Box<T>> {
        self.to_any().downcast::<T>().ok()
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
    /// Isize
    Isize(isize),
    /// U32, 32-bit unsigned integer
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

impl Pmt {
    /// Get the PMT variant kind without associated data.
    pub fn kind(&self) -> PmtKind {
        match self {
            Pmt::Ok => PmtKind::Ok,
            Pmt::InvalidValue => PmtKind::InvalidValue,
            Pmt::Null => PmtKind::Null,
            Pmt::String(_) => PmtKind::String,
            Pmt::Bool(_) => PmtKind::Bool,
            Pmt::Usize(_) => PmtKind::Usize,
            Pmt::Isize(_) => PmtKind::Isize,
            Pmt::U32(_) => PmtKind::U32,
            Pmt::U64(_) => PmtKind::U64,
            Pmt::F32(_) => PmtKind::F32,
            Pmt::F64(_) => PmtKind::F64,
            Pmt::VecCF32(_) => PmtKind::VecCF32,
            Pmt::VecF32(_) => PmtKind::VecF32,
            Pmt::VecU64(_) => PmtKind::VecU64,
            Pmt::Blob(_) => PmtKind::Blob,
            Pmt::VecPmt(_) => PmtKind::VecPmt,
            Pmt::Finished => PmtKind::Finished,
            Pmt::MapStrPmt(_) => PmtKind::MapStrPmt,
            Pmt::Any(_) => PmtKind::Any,
        }
    }
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
            Pmt::Isize(v) => write!(f, "{}", v),
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
            (Pmt::Isize(x), Pmt::Isize(y)) => x == y,
            (Pmt::U32(x), Pmt::U32(y)) => x == y,
            (Pmt::U64(x), Pmt::U64(y)) => x == y,
            (Pmt::F32(x), Pmt::F32(y)) => x == y,
            (Pmt::F64(x), Pmt::F64(y)) => x == y,
            (Pmt::VecF32(x), Pmt::VecF32(y)) => x == y,
            (Pmt::VecU64(x), Pmt::VecU64(y)) => x == y,
            (Pmt::VecCF32(x), Pmt::VecCF32(y)) => x == y,
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
#[error("PMT conversion error")]
pub struct PmtConversionError;

impl TryFrom<&Pmt> for f64 {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<f64, Self::Error> {
        match value {
            Pmt::F32(f) => Ok(*f as f64),
            Pmt::F64(f) => Ok(*f),
            Pmt::U32(f) => Ok(*f as f64),
            Pmt::U64(f) => Ok(*f as f64),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for f64 {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<f64, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&Pmt> for usize {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<usize, Self::Error> {
        match value {
            Pmt::Usize(f) => Ok(*f),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for usize {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<usize, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&Pmt> for isize {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<isize, Self::Error> {
        match value {
            Pmt::Isize(f) => Ok(*f),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for isize {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<isize, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&Pmt> for u64 {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<u64, Self::Error> {
        match value {
            Pmt::U32(v) => Ok(*v as u64),
            Pmt::U64(v) => Ok(*v),
            Pmt::Usize(v) => Ok(*v as u64),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for u64 {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<u64, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&Pmt> for bool {
    type Error = PmtConversionError;

    fn try_from(value: &Pmt) -> Result<bool, Self::Error> {
        match value {
            Pmt::Bool(b) => Ok(*b),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for bool {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<bool, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<Pmt> for Vec<f32> {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<Vec<f32>, Self::Error> {
        match value {
            Pmt::VecF32(v) => Ok(v),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for Vec<Complex32> {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<Vec<Complex32>, Self::Error> {
        match value {
            Pmt::VecCF32(v) => Ok(v),
            _ => Err(PmtConversionError),
        }
    }
}

impl TryFrom<Pmt> for Vec<u64> {
    type Error = PmtConversionError;

    fn try_from(value: Pmt) -> Result<Vec<u64>, Self::Error> {
        match value {
            Pmt::VecU64(v) => Ok(v),
            _ => Err(PmtConversionError),
        }
    }
}

impl From<()> for Pmt {
    fn from(_: ()) -> Self {
        Pmt::Null
    }
}

impl From<bool> for Pmt {
    fn from(b: bool) -> Self {
        Pmt::Bool(b)
    }
}

impl From<f32> for Pmt {
    fn from(f: f32) -> Self {
        Pmt::F32(f)
    }
}

impl From<f64> for Pmt {
    fn from(f: f64) -> Self {
        Pmt::F64(f)
    }
}

impl From<u32> for Pmt {
    fn from(f: u32) -> Self {
        Pmt::U32(f)
    }
}

impl From<u64> for Pmt {
    fn from(f: u64) -> Self {
        Pmt::U64(f)
    }
}

impl From<usize> for Pmt {
    fn from(f: usize) -> Self {
        Pmt::Usize(f)
    }
}

impl From<isize> for Pmt {
    fn from(f: isize) -> Self {
        Pmt::Isize(f)
    }
}

impl From<Vec<f32>> for Pmt {
    fn from(v: Vec<f32>) -> Self {
        Pmt::VecF32(v)
    }
}

impl From<Vec<u64>> for Pmt {
    fn from(v: Vec<u64>) -> Self {
        Pmt::VecU64(v)
    }
}

impl From<Vec<Complex32>> for Pmt {
    fn from(v: Vec<Complex32>) -> Self {
        Pmt::VecCF32(v)
    }
}

/// PMT types that do not wrap values.
///
/// Useful for bindings to other languages that do not support Rust's broad enum features.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// Isize
    Isize,
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

impl From<Pmt> for PmtKind {
    fn from(value: Pmt) -> Self {
        value.kind()
    }
}

impl From<&Pmt> for PmtKind {
    fn from(value: &Pmt) -> Self {
        value.kind()
    }
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
            PmtKind::Isize => write!(f, "isize"),
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
            "Isize" => return Ok(PmtKind::Isize),
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

        let fv1 = Pmt::VecF32(vec![1.0, 2.0, 3.0]);
        let fv2 = Pmt::VecF32(vec![1.0, 2.0, 3.0]);
        assert_eq!(fv1, fv2);

        let cfv1 = Pmt::VecCF32(vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)]);
        let cfv2 = Pmt::VecCF32(vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)]);
        assert_eq!(cfv1, cfv2);
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

    #[test]
    fn from_into() {
        let e = 42usize;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::Usize(e));
        assert_eq!((&p).try_into(), Ok(e));

        let e = 42isize;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::Isize(e));
        assert_eq!((&p).try_into(), Ok(e));

        let e = 42u32;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::U32(e));
        // Lossy conversion unsupported
        // assert_eq!(p.try_into(), Ok(e));

        let e = 42u64;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::U64(e));
        assert_eq!((&p).try_into(), Ok(e));

        let e = 42f64;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::F64(e));
        assert_eq!(p.try_into(), Ok(e));

        let e = 42f32;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::F32(e));
        // Lossy conversion unsupported
        //assert_eq!(p.try_into(), Ok(e));

        let e = true;
        let p = Pmt::from(e);
        assert_eq!(p, Pmt::Bool(e));
        assert_eq!((&p).try_into(), Ok(e));

        let e = vec![1.0, 2.0, 3.0];
        let p = Pmt::from(e.clone());
        assert_eq!(p, Pmt::VecF32(e.clone()));
        assert_eq!(p.try_into(), Ok(e));

        let e = vec![1, 2, 3];
        let p = Pmt::from(e.clone());
        assert_eq!(p, Pmt::VecU64(e.clone()));
        assert_eq!(p.try_into(), Ok(e));

        let e = vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)];
        let p = Pmt::from(e.clone());
        assert_eq!(p, Pmt::VecCF32(e.clone()));
        assert_eq!(p.try_into(), Ok(e));
    }

    #[test]
    fn pmt_kind() {
        let p = Pmt::U32(42);
        assert_eq!(PmtKind::U32, p.kind());
        assert_eq!(PmtKind::U32, p.into());

        let p = Pmt::F64(42.0);
        assert_eq!(PmtKind::F64, p.kind());
        assert_eq!(PmtKind::F64, p.into());

        let p = Pmt::VecF32(vec![]);
        assert_eq!(PmtKind::VecF32, p.kind());
        assert_eq!(PmtKind::VecF32, p.into());

        let p = Pmt::VecU64(vec![]);
        assert_eq!(PmtKind::VecU64, p.kind());
        assert_eq!(PmtKind::VecU64, (&p).into());

        let p = Pmt::VecCF32(vec![]);
        assert_eq!(PmtKind::VecCF32, p.kind());
        assert_eq!(PmtKind::VecCF32, (&p).into());
    }

    #[test]
    fn take_any() {
        let p = Pmt::Any(Box::new(vec![1u8]));

        let Pmt::Any(p_any) = p else { unreachable!() };
        assert!(p_any.is::<Vec<u8>>());

        let v = p_any.take::<Vec<u8>>().unwrap();
        assert_eq!(v[0], 1u8)
    }
}
