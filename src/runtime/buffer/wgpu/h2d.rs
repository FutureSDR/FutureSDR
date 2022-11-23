use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::wgpu::InputBufferEmpty as BufferEmpty;
use crate::runtime::buffer::wgpu::InputBufferFull as BufferFull;
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderCustom;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;

#[derive(Debug, PartialEq, Hash)]
pub struct H2D;

impl Eq for H2D {}

impl H2D {
    pub fn new() -> H2D {
        H2D
    }
}

impl Default for H2D {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for H2D {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        WriterH2D::new(item_size, writer_inbox, writer_output_id)
    }
}

// everything is measured in items, e.g., offsets, capacity, space available

// ====================== WRITER ============================
#[derive(Debug)]
pub struct WriterH2D {
    buffer: Option<CurrentBuffer>,
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    item_size: usize,
    finished: bool,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_input_id: Option<usize>,
}

#[derive(Debug)]
struct CurrentBuffer {
    buffer: BufferEmpty,
    offset: usize,
}

impl WriterH2D {
    pub fn new(
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        debug!("H2D writer created");

        BufferWriter::Host(Box::new(WriterH2D {
            buffer: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            item_size,
            finished: false,
            writer_inbox,
            writer_output_id,
            reader_inbox: None,
            reader_input_id: None,
        }))
    }
}

#[async_trait]
impl BufferWriterHost for WriterH2D {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        debug!("H2D writer called add reader");
        debug_assert!(self.reader_inbox.is_none());
        debug_assert!(self.reader_input_id.is_none());

        self.reader_inbox = Some(reader_inbox);
        self.reader_input_id = Some(reader_input_id);

        debug_assert_eq!(reader_input_id, 0);
        BufferReader::Custom(Box::new(ReaderH2D {
            inbound: self.outbound.clone(),
            outbound: self.inbound.clone(),
            writer_inbox: self.writer_inbox.clone(),
            writer_output_id: self.writer_output_id,
            finished: false,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        if self.buffer.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            } else {
                debug!("H2D writer called bytes, buff is none");
                return (std::ptr::null_mut::<u8>(), 0);
            }
        }

        unsafe {
            let buffer = self.buffer.as_mut().unwrap();
            let capacity = buffer.buffer.buffer.len() / self.item_size;
            let ret = buffer.buffer.buffer.as_mut_ptr();
            (
                ret.add(buffer.offset * self.item_size),
                (capacity - buffer.offset) * self.item_size,
            )
        }
    }

    fn produce(&mut self, amount: usize, _tags: Vec<ItemTag>) {
        debug!("H2D writer called produce {}", amount);
        let buffer = self.buffer.as_mut().unwrap();
        let capacity = buffer.buffer.buffer.len() / self.item_size;

        debug_assert!(amount + buffer.offset <= capacity);
        buffer.offset += amount;
        if buffer.offset == capacity {
            let buffer = self.buffer.take().unwrap().buffer.buffer;
            self.outbound.lock().unwrap().push_back(BufferFull {
                buffer,
                used_bytes: capacity * self.item_size,
            });

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            }

            let _ = self
                .reader_inbox
                .as_mut()
                .unwrap()
                .try_send(BlockMessage::Notify);
        }
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");
        if self.finished {
            return;
        }

        if let Some(CurrentBuffer { offset, buffer }) = self.buffer.take() {
            if offset > 0 {
                self.outbound.lock().unwrap().push_back(BufferFull {
                    buffer: buffer.buffer,
                    used_bytes: offset * self.item_size,
                });
            }
        }

        self.reader_inbox
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamInputDone {
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

// ====================== READER ============================
#[derive(Debug)]
pub struct ReaderH2D {
    inbound: Arc<Mutex<VecDeque<BufferFull>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty>>>,
    writer_output_id: usize,
    writer_inbox: Sender<BlockMessage>,
    finished: bool,
}

impl ReaderH2D {
    pub fn submit(&mut self, buffer: BufferEmpty) {
        debug!("H2D reader handling empty buffer");
        self.outbound.lock().unwrap().push(buffer);
        let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    }

    pub fn get_buffer(&mut self) -> Option<BufferFull> {
        let mut vec = self.inbound.lock().unwrap();
        vec.pop_front()
    }
}

#[async_trait]
impl BufferReaderCustom for ReaderH2D {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn notify_finished(&mut self) {
        debug!("H2D reader finish");
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(BlockMessage::StreamOutputDone {
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
