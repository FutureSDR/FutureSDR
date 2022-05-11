use futures::channel::mpsc::Sender;
use std::fmt;
use std::mem;
use std::slice;

use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::tag::default_tag_propagation;
use crate::runtime::AsyncMessage;
use crate::runtime::Tag;
use crate::runtime::ItemTag;
use crate::runtime::TagOutputQueue;

#[derive(Debug)]
struct CurrentInput {
    ptr: *const u8,
    len: usize,
    index: usize,
}

#[derive(Debug)]
pub struct StreamInput {
    name: String,
    item_size: usize,
    reader: Option<BufferReader>,
    current: Option<CurrentInput>,
    tags: Vec<ItemTag>,
}

unsafe impl Send for StreamInput {}

impl StreamInput {
    pub fn new(name: &str, item_size: usize) -> StreamInput {
        StreamInput {
            name: name.to_string(),
            item_size,
            reader: None,
            current: None,
            tags: Vec::new(),
        }
    }

    pub fn item_size(&self) -> usize {
        self.item_size
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn try_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.reader.as_mut().unwrap().try_as::<T>()
    }

    pub fn consume(&mut self, amount: usize) {
        debug_assert!(self.current.is_some());
        debug_assert!(amount <= self.current.as_mut().unwrap().len - self.current.as_mut().unwrap().index * self.item_size);

        self.current.as_mut().unwrap().index += amount * self.item_size;
        self.tags.retain(|x| x.index >= amount);
    }

    pub fn slice<T>(&mut self) -> &'static [T] {
        if self.current.is_none() {
            let (ptr, len, tags) = self.reader.as_mut().unwrap().bytes();
            self.current = Some(CurrentInput {
                ptr,
                len,
                index: 0,
            });
            self.tags = tags;
        } 

        let c = self.current.as_ref().unwrap();
        unsafe { slice::from_raw_parts(c.ptr as *const T, c.len / mem::size_of::<T>()) }
    }

    pub fn tags(&mut self) -> &mut Vec<ItemTag> {
        &mut self.tags
    }

    fn commit(&mut self) {
        if let Some(ref c) = self.current {
            let amount = c.index / self.item_size;
            if amount != 0 {
                self.reader.as_mut().unwrap().consume(amount);
            }
            self.current = None;
        }
    }

    pub fn set_reader(&mut self, reader: BufferReader) {
        debug_assert!(self.reader.is_none());
        self.reader = Some(reader);
    }

    pub async fn notify_finished(&mut self) {
        self.reader.as_mut().unwrap().notify_finished().await;
    }

    pub fn finish(&mut self) {
        self.reader.as_mut().unwrap().finish();
    }

    pub fn finished(&self) -> bool {
        self.reader.as_ref().unwrap().finished()
    }
}

#[derive(Debug)]
pub struct StreamOutput {
    name: String,
    item_size: usize,
    writer: Option<BufferWriter>,
    tags: TagOutputQueue,
    offset: usize,
}

impl StreamOutput {
    pub fn new(name: &str, item_size: usize) -> StreamOutput {
        StreamOutput {
            name: name.to_string(),
            item_size,
            writer: None,
            tags: TagOutputQueue::new(),
            offset: 0,
        }
    }

    pub fn item_size(&self) -> usize {
        self.item_size
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn init(&mut self, writer: BufferWriter) {
        debug_assert!(self.writer.is_none());
        self.writer = Some(writer);
    }

    pub fn add_tag(&mut self, index: usize, tag: Tag) {
        self.tags.add(index + self.offset, tag);
    }

    pub fn add_reader(
        &mut self,
        reader_inbox: Sender<AsyncMessage>,
        reader_port: usize,
    ) -> BufferReader {
        debug_assert!(self.writer.is_some());
        self.writer
            .as_mut()
            .unwrap()
            .add_reader(reader_inbox, reader_port)
    }

    pub fn try_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.writer.as_mut().unwrap().try_as::<T>()
    }

    pub fn produce(&mut self, amount: usize) {
        self.offset += amount;
    }

    pub fn slice<T>(&mut self) -> &'static mut [T] {
        let (ptr, len) = self.writer.as_mut().unwrap().bytes();

        unsafe {
            slice::from_raw_parts_mut(
                ptr.cast::<T>().add(self.offset),
                (len / mem::size_of::<T>()) - self.offset,
            )
        }
    }

    fn commit(&mut self) {
        if self.offset == 0 {
            return;
        }
        let tags = self.tags.produce(self.offset);
        self.writer.as_mut().unwrap().produce(self.offset, tags);
        self.offset = 0;
    }

    pub async fn notify_finished(&mut self) {
        self.writer.as_mut().unwrap().notify_finished().await;
    }

    pub fn finish(&mut self) {
        self.writer.as_mut().unwrap().finish();
    }

    pub fn finished(&self) -> bool {
        self.writer.as_ref().unwrap().finished()
    }

    pub(super) fn writer_mut(&mut self) -> &mut BufferWriter {
        let w = self.writer.as_mut().unwrap();
        w
    }
}

pub struct StreamIo {
    inputs: Vec<StreamInput>,
    outputs: Vec<StreamOutput>,
    tag_propagation: Box<dyn FnMut(&mut Vec<StreamInput>, &mut Vec<StreamOutput>) + Send + 'static>,
}

impl fmt::Debug for StreamIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamIo")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .finish()
    }
}

impl StreamIo {
    fn new(
        inputs: Vec<StreamInput>,
        outputs: Vec<StreamOutput>,
        tag_propagation: Box<
            dyn FnMut(&mut Vec<StreamInput>, &mut Vec<StreamOutput>) + Send + 'static,
        >,
    ) -> StreamIo {
        StreamIo {
            inputs,
            outputs,
            tag_propagation,
        }
    }

    pub fn inputs(&self) -> &Vec<StreamInput> {
        &self.inputs
    }

    pub fn inputs_mut(&mut self) -> &mut Vec<StreamInput> {
        &mut self.inputs
    }

    pub fn input_by_name(&self, name: &str) -> Option<&StreamInput> {
        self.inputs.iter().find(|x| x.name() == name)
    }

    pub fn input_by_name_mut(&mut self, name: &str) -> Option<&mut StreamInput> {
        self.inputs.iter_mut().find(|x| x.name() == name)
    }

    pub fn input_ref(&self, id: usize) -> &StreamInput {
        &self.inputs[id]
    }

    pub fn input(&mut self, id: usize) -> &mut StreamInput {
        &mut self.inputs[id]
    }

    pub fn input_name_to_id(&self, name: &str) -> Option<usize> {
        self.inputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    pub fn outputs(&self) -> &Vec<StreamOutput> {
        &self.outputs
    }

    pub fn outputs_mut(&mut self) -> &mut Vec<StreamOutput> {
        &mut self.outputs
    }

    pub fn output_by_name(&self, name: &str) -> Option<&StreamOutput> {
        self.outputs.iter().find(|x| x.name() == name)
    }

    pub fn output_by_name_mut(&mut self, name: &str) -> Option<&mut StreamOutput> {
        self.outputs.iter_mut().find(|x| x.name() == name)
    }

    pub fn output_ref(&self, id: usize) -> &StreamOutput {
        &self.outputs[id]
    }

    pub fn output(&mut self, id: usize) -> &mut StreamOutput {
        &mut self.outputs[id]
    }

    pub fn output_name_to_id(&self, name: &str) -> Option<usize> {
        self.outputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    pub fn commmit(&mut self) {
        (self.tag_propagation)(&mut self.inputs, &mut self.outputs);
        for i in self.inputs_mut() {
            i.commit();
        }
        for o in self.outputs_mut() {
            o.commit();
        }
    }

    pub fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut Vec<StreamInput>, &mut Vec<StreamOutput>) + Send + 'static>,
    ) {
        self.set_tag_propagation(f);
    }
}

pub struct StreamIoBuilder {
    inputs: Vec<StreamInput>,
    outputs: Vec<StreamOutput>,
    tag_propagation: Box<dyn FnMut(&mut Vec<StreamInput>, &mut Vec<StreamOutput>) + Send + 'static>,
}

impl StreamIoBuilder {
    pub fn new() -> StreamIoBuilder {
        StreamIoBuilder {
            inputs: Vec::new(),
            outputs: Vec::new(),
            tag_propagation: Box::new(default_tag_propagation),
        }
    }

    #[must_use]
    pub fn add_input(mut self, name: &str, item_size: usize) -> StreamIoBuilder {
        self.inputs.push(StreamInput::new(name, item_size));
        self
    }

    #[must_use]
    pub fn add_output(mut self, name: &str, item_size: usize) -> StreamIoBuilder {
        self.outputs.push(StreamOutput::new(name, item_size));
        self
    }

    #[must_use]
    pub fn tag_propagation<
        F: FnMut(&mut Vec<StreamInput>, &mut Vec<StreamOutput>) + Send + 'static,
    >(
        mut self,
        f: F,
    ) -> StreamIoBuilder {
        self.tag_propagation = Box::new(f);
        self
    }

    pub fn build(self) -> StreamIo {
        StreamIo::new(self.inputs, self.outputs, self.tag_propagation)
    }
}

impl Default for StreamIoBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_connect() {
        let i = StreamInput::new("foo", 4);
        assert_eq!(i.name(), "foo");
        assert_eq!(i.item_size(), 4);

        let o = StreamOutput::new("foo", 4);
        assert_eq!(o.name(), "foo");
        assert_eq!(o.item_size(), 4);
    }
}
