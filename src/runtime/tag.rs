use std::collections::VecDeque;

use crate::runtime::Pmt;
use crate::runtime::StreamInput;
use crate::runtime::StreamOutput;

#[derive(Clone, Debug)]
pub enum Tag {
    Id(u64),
    String(String),
    Data(Pmt),
}

#[derive(Clone, Debug)]
pub struct ItemTag {
    pub index: usize,
    pub tag: Tag,
}

pub fn default_tag_propagation(_inputs: &mut [StreamInput], _outputs: &mut [StreamOutput]) {}

#[derive(Debug)]
pub struct TagOutputQueue {
    queue: VecDeque<ItemTag>,
}

impl TagOutputQueue {
    pub fn new() -> Self {
        TagOutputQueue {
            queue: VecDeque::new(),
        }
    }

    pub fn add(&mut self, index: usize, tag: Tag) {
        self.queue.push_back(ItemTag { index, tag });
    }

    pub fn produce(&mut self, samples: usize) -> Vec<ItemTag> {
        debug_assert!(samples > 0);
        let tags = self
            .queue
            .iter()
            .filter(|x| x.index < samples)
            .cloned()
            .collect();

        self.queue.retain(|x| x.index >= samples);

        for t in self.queue.iter_mut() {
            t.index -= samples;
        }

        tags
    }
}

impl Default for TagOutputQueue {
    fn default() -> Self {
        Self::new()
    }
}
