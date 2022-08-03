use crate::runtime::Pmt;
use crate::runtime::StreamInput;
use crate::runtime::StreamOutput;

#[derive(Clone, Debug)]
pub enum Tag {
    Id(u64),
    String(String),
    Data(Pmt),
    NamedF32(String, f32),
}

#[derive(Clone, Debug)]
pub struct ItemTag {
    pub index: usize,
    pub tag: Tag,
}

pub fn default_tag_propagation(_inputs: &mut [StreamInput], _outputs: &mut [StreamOutput]) {}
