use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::debug;
use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::config;
use crate::runtime::AsyncMessage;

#[derive(Debug, PartialEq, Hash)]
pub struct Slab {
    min_bytes: usize,
}

impl Eq for Slab {}

impl Slab {
    pub fn new() -> Slab {
        Slab {
            min_bytes: config::config().buffer_size,
        }
    }

    pub fn with_size(min_bytes: usize) -> Slab {
        Slab { min_bytes }
    }
}

impl Default for Slab {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for Slab {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        Writer::new(item_size, self.min_bytes, writer_inbox, writer_output_id)
    }
}

// everything is measured in items, e.g., offsets, capacity, space available

#[derive(Debug)]
pub struct Writer {
    buffer: Box<[u8]>,
    state: Arc<Mutex<State>>,
    capacity: usize,
    item_size: usize,
    reader_inbox: Option<Sender<AsyncMessage>>,
    reader_input_id: Option<usize>,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
    finished: bool,
}

#[derive(Debug)]
struct State {
    writer_offset: usize,
    reader_offset: usize,
    full: bool,
}

impl Writer {
    pub fn new(
        item_size: usize,
        min_bytes: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        let mut buffer_size = min_bytes;
        while buffer_size % item_size != 0 {
            buffer_size += 1;
        }

        debug!("write with size {:?}", buffer_size);
        BufferWriter::Host(Box::new(Writer {
            buffer: vec![0; buffer_size].into_boxed_slice(),
            state: Arc::new(Mutex::new(State {
                writer_offset: 0,
                reader_offset: 0,
                full: false,
            })),
            capacity: buffer_size / item_size,
            item_size,
            reader_inbox: None,
            reader_input_id: None,
            writer_inbox,
            writer_output_id,
            finished: false,
        }))
    }

    fn space_available(&self) -> usize {
        let state = self.state.lock().unwrap();

        if state.full {
            debug_assert_eq!(state.writer_offset, state.reader_offset);
            return 0;
        }

        if state.reader_offset > state.writer_offset {
            state.reader_offset - state.writer_offset
        } else {
            self.capacity - state.writer_offset
        }
    }
}

#[async_trait]
impl BufferWriterHost for Writer {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<AsyncMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        debug_assert!(self.reader_inbox.is_none());
        debug_assert!(self.reader_input_id.is_none());

        self.reader_inbox = Some(reader_inbox);
        self.reader_input_id = Some(reader_input_id);

        let mut state = self.state.lock().unwrap();
        state.reader_offset = state.writer_offset;

        BufferReader::Host(Box::new(Reader {
            ptr: self.buffer.as_ptr(),
            state: self.state.clone(),
            capacity: self.capacity,
            item_size: self.item_size,
            writer_inbox: self.writer_inbox.clone(),
            writer_output_id: self.writer_output_id,
            finished: false,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        let space = self.space_available();
        let state = self.state.lock().unwrap();
        debug!(
            "write handing out n items {:?}, offset {:?}",
            space, state.writer_offset
        );

        unsafe {
            (
                self.buffer
                    .as_mut_ptr()
                    .add(state.writer_offset * self.item_size),
                space * self.item_size,
            )
        }
    }

    fn produce(&mut self, amount: usize) {
        debug_assert!(amount <= self.space_available());

        let mut state = self.state.lock().unwrap();

        state.writer_offset = (state.writer_offset + amount) % self.capacity;

        if state.reader_offset == state.writer_offset {
            state.full = true;
        }

        debug!(
            "write producing {:?}, new writer offset {:?}, full {:?}",
            amount, state.writer_offset, state.full
        );

        let _ = self
            .reader_inbox
            .as_mut()
            .unwrap()
            .try_send(AsyncMessage::Notify);
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        self.reader_inbox
            .as_mut()
            .unwrap()
            .send(AsyncMessage::StreamInputDone {
                input_id: self.reader_input_id.unwrap(),
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

unsafe impl Send for Writer {}

#[derive(Debug)]
pub struct Reader {
    ptr: *const u8,
    state: Arc<Mutex<State>>,
    capacity: usize,
    item_size: usize,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
    finished: bool,
}

impl Reader {
    fn space_available(&self) -> usize {
        let state = self.state.lock().unwrap();

        if state.full {
            debug_assert_eq!(state.reader_offset, state.writer_offset);
            return self.capacity;
        }

        if state.reader_offset > state.writer_offset {
            self.capacity - state.reader_offset
        } else {
            state.writer_offset - state.reader_offset
        }
    }
}

#[async_trait]
impl BufferReaderHost for Reader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*const u8, usize) {
        let space = self.space_available();

        let state = self.state.lock().unwrap();
        debug!(
            "reader handing out n items {:?}, offset {:?}",
            space, state.reader_offset
        );
        unsafe {
            (
                self.ptr.add(state.reader_offset * self.item_size),
                space * self.item_size,
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        debug_assert!(amount <= self.space_available());
        let mut state = self.state.lock().unwrap();

        state.reader_offset = (state.reader_offset + amount) % self.capacity;
        if amount > 0 {
            state.full = false;
        }

        debug!(
            "reader consuming {:?}, new read offset {:?}, full {:?}",
            amount, state.reader_offset, state.full
        );

        let _ = self.writer_inbox.try_send(AsyncMessage::Notify);
    }

    async fn notify_finished(&mut self) {
        debug!("Slab Reader notifies writer");
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(AsyncMessage::StreamOutputDone {
                output_id: self.writer_output_id,
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

unsafe impl Send for Reader {}
