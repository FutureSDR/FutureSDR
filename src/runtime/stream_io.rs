use futures::channel::mpsc::Sender;
use std::mem;
use std::slice;

use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::AsyncMessage;

#[derive(Debug)]
pub struct StreamInput {
    name: String,
    item_size: usize,
    reader: Option<BufferReader>,
}

impl StreamInput {
    pub fn new(name: &str, item_size: usize) -> StreamInput {
        StreamInput {
            name: name.to_string(),
            item_size,
            reader: None,
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
        if amount == 0 {
            return;
        }
        self.reader.as_mut().unwrap().consume(amount);
    }

    pub fn slice<T>(&mut self) -> &'static mut [T] {
        let (ptr, len) = self.reader.as_mut().unwrap().bytes();

        unsafe { slice::from_raw_parts_mut(ptr as *mut T, len / mem::size_of::<T>()) }
    }

    pub fn as_slice<T>(&mut self) -> &'static [T] {
        let (ptr, len) = self.reader.as_mut().unwrap().bytes();

        unsafe { slice::from_raw_parts(ptr as *const T, len / mem::size_of::<T>()) }
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
}

impl StreamOutput {
    pub fn new(name: &str, item_size: usize) -> StreamOutput {
        StreamOutput {
            name: name.to_string(),
            item_size,
            writer: None,
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
        if amount == 0 {
            return;
        }
        self.writer.as_mut().unwrap().produce(amount)
    }

    pub fn slice<T>(&mut self) -> &'static mut [T] {
        let (ptr, len) = self.writer.as_mut().unwrap().bytes();

        unsafe { slice::from_raw_parts_mut(ptr.cast::<T>(), len / mem::size_of::<T>()) }
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
}

#[derive(Debug)]
pub struct StreamIo {
    inputs: Vec<StreamInput>,
    outputs: Vec<StreamOutput>,
}

impl StreamIo {
    fn new(inputs: Vec<StreamInput>, outputs: Vec<StreamOutput>) -> StreamIo {
        StreamIo { inputs, outputs }
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
}

pub struct StreamIoBuilder {
    inputs: Vec<StreamInput>,
    outputs: Vec<StreamOutput>,
}

impl StreamIoBuilder {
    pub fn new() -> StreamIoBuilder {
        StreamIoBuilder {
            inputs: Vec::new(),
            outputs: Vec::new(),
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

    pub fn build(self) -> StreamIo {
        StreamIo::new(self.inputs, self.outputs)
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
