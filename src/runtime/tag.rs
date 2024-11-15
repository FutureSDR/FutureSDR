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

/// Item tag
#[derive(Clone, Debug)]
pub struct ItemTag {
    /// Index of sample in buffer
    pub index: usize,
    /// [`Tag`] value
    pub tag: Tag,
}

/// No-op tag propagation strategy.
pub fn default_tag_propagation(_inputs: &mut [StreamInput], _outputs: &mut [StreamOutput]) {}

/// Tag propagation strategy where all tags are copied from input to output.
///
/// # Note
///
/// Assumes `inputs[..].consumed() == outputs[..].produced()`
///
/// # Example
///
/// ```rust, no_run
/// # use futuresdr::blocks::Fft;
/// # use futuresdr::runtime::copy_tag_propagation;
/// let mut fft = Fft::new(1024);
/// fft.set_tag_propagation(Box::new(copy_tag_propagation));
/// ```
pub fn copy_tag_propagation(inputs: &mut [StreamInput], outputs: &mut [StreamOutput]) {
    debug_assert_eq!(inputs[0].consumed().0, outputs[0].produced());
    let (n, tags) = inputs[0].consumed();
    for t in tags.iter().filter(|x| x.index < n) {
        outputs[0].add_tag_abs(t.index, t.tag.clone());
    }
}
