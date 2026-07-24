use dyn_clone::DynClone;
use std::any::Any;
use std::fmt;

use crate::runtime::Pmt;

/// Trait object support for arbitrary stream tag payloads.
pub trait TagAny: Any + DynClone + Send + Sync + 'static {
    /// Return whether this payload equals another type-erased tag payload.
    fn eq_any(&self, other: &dyn TagAny) -> bool;
}
dyn_clone::clone_trait_object!(TagAny);

impl<T: Any + DynClone + PartialEq + Send + Sync + 'static> TagAny for T {
    fn eq_any(&self, other: &dyn TagAny) -> bool {
        (other as &dyn Any).downcast_ref::<T>() == Some(self)
    }
}

impl dyn TagAny {
    /// Return whether the boxed tag payload has type `T`.
    pub fn is<T: TagAny>(&self) -> bool {
        (self as &dyn Any).is::<T>()
    }
    /// Downcast the boxed tag payload by shared reference.
    pub fn downcast_ref<T: TagAny>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
    /// Downcast the boxed tag payload by mutable reference.
    pub fn downcast_mut<T: TagAny>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }
}

impl fmt::Debug for Box<dyn TagAny> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Box<dyn Any>")
    }
}

/// Metadata attached to an item in a stream.
///
/// Tags travel with stream items and can carry simple built-in values, PMTs, or
/// cloneable type-erased payloads.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Tag {
    /// Numeric identifier.
    Id(u64),
    /// String payload.
    String(String),
    /// PMT payload.
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
        match (self, other) {
            (Tag::Id(x), Tag::Id(y)) => x == y,
            (Tag::String(x), Tag::String(y)) => x == y,
            (Tag::Data(x), Tag::Data(y)) => x == y,
            (Tag::NamedUsize(k1, v1), Tag::NamedUsize(k2, v2)) => k1 == k2 && v1 == v2,
            (Tag::NamedF32(k1, v1), Tag::NamedF32(k2, v2)) => k1 == k2 && v1 == v2,
            (Tag::NamedAny(k1, v1), Tag::NamedAny(k2, v2)) => k1 == k2 && v1.eq_any(&**v2),
            (Tag::Id(_), _)
            | (Tag::String(_), _)
            | (Tag::Data(_), _)
            | (Tag::NamedUsize(_, _), _)
            | (Tag::NamedF32(_, _), _)
            | (Tag::NamedAny(_, _), _) => false,
        }
    }
}

/// A stream tag with the item index it applies to.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTag {
    /// Index of the tagged item in the buffer.
    pub index: usize,
    /// Tag value.
    pub tag: Tag,
}
