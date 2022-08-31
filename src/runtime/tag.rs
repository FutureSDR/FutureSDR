use dyn_clone::DynClone;
use std::any::Any;
use std::fmt;

use crate::runtime::Pmt;
use crate::runtime::StreamInput;
use crate::runtime::StreamOutput;

pub trait TagAny: Any + DynClone + Send + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
dyn_clone::clone_trait_object!(TagAny);

impl<T: Any + DynClone + Send + 'static> TagAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn TagAny {
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

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Tag {
    Id(u64),
    String(String),
    Data(Pmt),
    NamedUsize(String, usize),
    NamedF32(String, f32),
    NamedAny(String, Box<dyn TagAny>),
}

#[derive(Clone, Debug)]
pub struct ItemTag {
    pub index: usize,
    pub tag: Tag,
}

pub fn default_tag_propagation(_inputs: &mut [StreamInput], _outputs: &mut [StreamOutput]) {}
