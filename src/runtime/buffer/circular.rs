use futures::channel::mpsc::Sender;
use futures::prelude::*;
use slab::Slab;
use std::any::Any;
use std::cmp;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::pagesize;
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::buffer::DoubleMapped;
use crate::runtime::config;
use crate::runtime::AsyncMessage;

// everything is measured in items, e.g., offsets, capacity, space available

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Circular {
    min_bytes: usize,
}

impl Eq for Circular {}

impl Circular {
    pub fn new() -> Circular {
        Circular {
            min_bytes: config::config().buffer_size,
        }
    }
    pub fn with_size(min_bytes: usize) -> Circular {
        Circular { min_bytes }
    }
}

impl Default for Circular {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for Circular {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        BufferWriter::Host(Box::new(Writer::new(
            item_size,
            self.min_bytes,
            writer_inbox,
            writer_output_id,
        )))
    }
}

#[derive(Debug)]
pub struct Writer {
    buffer: DoubleMapped,
    state: Arc<Mutex<State>>,
    capacity: usize,
    item_size: usize,
    inbox: Sender<AsyncMessage>,
    output_id: usize,
    finished: bool,
}

#[async_trait]
impl BufferWriterHost for Writer {
    fn add_reader(&mut self, inbox: Sender<AsyncMessage>, input_id: usize) -> BufferReader {
        let mut state = self.state.lock().unwrap();
        let writer_offset = state.writer_offset;
        let id = state.readers.insert(ReaderState {
            offset: writer_offset,
            inbox,
            input_id,
        });

        BufferReader::Host(Box::new(Reader {
            ptr: self.buffer.addr(),
            state: self.state.clone(),
            capacity: self.capacity,
            item_size: self.item_size,
            finished: false,
            id,
            writer_inbox: self.inbox.clone(),
            writer_output_id: self.output_id,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn produce(&mut self, amount: usize) {
        debug_assert!(amount <= self.space_available().0);

        let mut state = self.state.lock().unwrap();

        state.writer_offset = (state.writer_offset + amount) % self.capacity;

        for (_, r) in state.readers.iter_mut() {
            // if the inbox is already full, there's no need to explicitly notify
            let _ = r.inbox.try_send(AsyncMessage::Notify);
        }
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        let (space, offset) = self.space_available();
        unsafe {
            (
                self.buffer.addr().add(offset * self.item_size) as *mut u8,
                space * self.item_size,
            )
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let mut readers: Vec<(Sender<AsyncMessage>, usize)> = {
            let mut state = self.state.lock().unwrap();
            state
                .readers
                .iter_mut()
                .map(|s| (s.1.inbox.clone(), s.1.input_id))
                .collect()
        };

        for i in readers.iter_mut() {
            i.0.send(AsyncMessage::StreamInputDone { input_id: i.1 })
                .await
                .unwrap();
        }
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

#[derive(Debug)]
struct State {
    writer_offset: usize,
    readers: Slab<ReaderState>,
}

#[derive(Debug)]
struct ReaderState {
    offset: usize,
    inbox: Sender<AsyncMessage>,
    input_id: usize,
}

impl Writer {
    pub fn new(
        item_size: usize,
        min_bytes: usize,
        inbox: Sender<AsyncMessage>,
        output_id: usize,
    ) -> Writer {
        let page_size = pagesize();
        let mut buffer_size = page_size;

        while (buffer_size < min_bytes) || (buffer_size % item_size != 0) {
            buffer_size += page_size;
        }

        Writer {
            buffer: DoubleMapped::new(buffer_size).unwrap(),
            state: Arc::new(Mutex::new(State {
                writer_offset: 0,
                readers: Slab::new(),
            })),
            capacity: buffer_size / item_size,
            item_size,
            inbox,
            output_id,
            finished: false,
        }
    }

    fn space_available(&self) -> (usize, usize) {
        let mut space = self.capacity;

        let state = self.state.lock().unwrap();

        for (_, reader) in state.readers.iter() {
            if reader.offset <= state.writer_offset {
                space = cmp::min(
                    space,
                    reader.offset + self.capacity - 1 - state.writer_offset,
                );
            } else {
                space = cmp::min(space, reader.offset - 1 - state.writer_offset);
            }
        }

        (space, state.writer_offset)
    }
}

unsafe impl Send for Writer {}
unsafe impl Sync for Writer {}

#[derive(Debug)]
pub struct Reader {
    ptr: *const std::ffi::c_void,
    state: Arc<Mutex<State>>,
    capacity: usize,
    item_size: usize,
    finished: bool,
    id: usize,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
}

impl Reader {
    fn space_available(read_offset: usize, write_offset: usize, capacity: usize) -> usize {
        if read_offset > write_offset {
            write_offset + capacity - read_offset
        } else {
            write_offset - read_offset
        }
    }
}

#[async_trait]
impl BufferReaderHost for Reader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*const u8, usize) {
        let state = self.state.lock().unwrap();
        let reader_offset = state.readers.get(self.id).unwrap().offset;
        let writer_offset = state.writer_offset;
        drop(state);

        let space = Self::space_available(reader_offset, writer_offset, self.capacity);

        unsafe {
            (
                self.ptr.add(reader_offset * self.item_size) as *const u8,
                space * self.item_size,
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        let mut state = self.state.lock().unwrap();
        let writer_offset = state.writer_offset;
        let reader = state.readers.get_mut(self.id).unwrap();

        debug_assert!({
            amount <= Self::space_available(reader.offset, writer_offset, self.capacity)
        });

        reader.offset = (reader.offset + amount) % self.capacity;
        drop(state);

        // if full, no need to notify
        let _ = self.writer_inbox.try_send(AsyncMessage::Notify);
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(AsyncMessage::StreamOutputDone {
                output_id: self.writer_output_id,
            })
            .await
            .unwrap();

        self.state.lock().unwrap().readers.remove(self.id);
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc::channel;
    use std::slice;

    #[test]
    fn circ_buffer() {
        async_io::block_on(async {
            let ps = pagesize();
            let item_size = 8;
            let (tx, _rx) = channel(1);
            let mut w = Writer::new(item_size, 123, tx, 0);

            assert_eq!(w.item_size, item_size);
            assert_eq!((w.capacity * item_size) % ps, 0);
            assert_eq!(w.state.lock().unwrap().readers.len(), 0);
            assert_eq!(w.bytes().1 / item_size, w.capacity);

            let (ri, _ro) = channel(100);

            let mut r = w.add_reader(ri, 0);
            assert_eq!(r.bytes().1, 0);
            assert_eq!(w.bytes().1 / item_size, w.capacity - 1);
            assert_eq!(w.state.lock().unwrap().readers.len(), 1);

            let (buff, size) = w.bytes();

            unsafe {
                let buff = slice::from_raw_parts_mut::<u64>(buff as *mut u64, size / item_size);
                for i in 0..10 {
                    buff[i] = i as u64;
                }
            }

            w.produce(3);
            w.produce(7);
            assert_eq!(r.bytes().1 / item_size, 10);
            assert_eq!(w.bytes().1 / item_size, w.capacity - 1 - 10);

            let (buff, size) = r.bytes();
            unsafe {
                let buff = slice::from_raw_parts_mut::<u64>(buff as *mut u64, size / item_size);
                for i in 0..r.bytes().1 / item_size {
                    assert!(buff[i] == i as u64);
                }
            }

            r.consume(6);
            assert_eq!(r.bytes().1 / item_size, 4);
            assert_eq!(w.bytes().1 / item_size, w.capacity - 1 - 4);
        });
    }
}
