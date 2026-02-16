use dyn_clone::DynClone;
use std::any::Any;
use std::fmt;

use crate::runtime::Pmt;

pub trait TagAny: Any + DynClone + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
dyn_clone::clone_trait_object!(TagAny);

impl<T: Any + DynClone + Send + Sync + 'static> TagAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn TagAny {
    pub fn is<T: TagAny>(&self) -> bool {
        self.as_any().is::<T>()
    }
    pub fn downcast_ref<T: TagAny>(&self) -> Option<&T> {
        (*self).as_any().downcast_ref::<T>()
    }
    pub fn downcast_mut<T: TagAny>(&mut self) -> Option<&mut T> {
        (*self).as_any_mut().downcast_mut::<T>()
    }
}

impl fmt::Debug for Box<dyn TagAny> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Box<dyn Any>")
    }
}

/// Stream tag
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Tag {
    /// Id
    Id(u64),
    /// String
    String(String),
    /// Pmt
    Data(Pmt),
    /// A `usize` with a name
    NamedUsize(String, usize),
    /// An `f32` with a name
    NamedF32(String, f32),
    /// Arbitrary data with a name
    NamedAny(String, Box<dyn TagAny>),
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Tag::Id(x) => match other {
                Tag::Id(y) => x == y,
                _ => false,
            },
            Tag::String(x) => match other {
                Tag::String(y) => x == y,
                _ => false,
            },
            Tag::Data(x) => match other {
                Tag::Data(y) => x == y,
                _ => false,
            },
            Tag::NamedUsize(k1, v1) => match other {
                Tag::NamedUsize(k2, v2) => k1 == k2 && v1 == v2,
                _ => false,
            },
            Tag::NamedF32(k1, v1) => match other {
                Tag::NamedF32(k2, v2) => k1 == k2 && v1 == v2,
                _ => false,
            },
            _ => false,
        }
    }
}

/// Item tag
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTag {
    /// Index of sample in buffer
    pub index: usize,
    /// [`Tag`] value
    pub tag: Tag,
}
