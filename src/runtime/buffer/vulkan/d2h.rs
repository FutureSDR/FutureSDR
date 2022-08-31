use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::vulkan::BufferEmpty;
use crate::runtime::buffer::vulkan::BufferFull;
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterCustom;
use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;

#[derive(Debug, PartialEq, Hash)]
pub struct D2H;

impl Eq for D2H {}

impl D2H {
    pub fn new() -> D2H {
        D2H
    }
}

impl Default for D2H {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for D2H {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        WriterD2H::new(item_size, writer_inbox, writer_output_id)
    }
}

// everything is measured in items, e.g., offsets, capacity, space available

#[derive(Debug)]
pub struct WriterD2H {
    item_size: usize,
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    finished: bool,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_input_id: Option<usize>,
}

impl WriterD2H {
    pub fn new(
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        BufferWriter::Custom(Box::new(WriterD2H {
            item_size,
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            finished: false,
            writer_inbox,
            writer_output_id,
            reader_inbox: None,
            reader_input_id: None,
        }))
    }

    pub fn buffers(&mut self) -> Vec<BufferEmpty> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    pub fn submit(&mut self, buffer: BufferFull) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self
            .reader_inbox
            .as_mut()
            .unwrap()
            .try_send(BlockMessage::Notify);
    }
}

#[async_trait]
impl BufferWriterCustom for WriterD2H {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        debug_assert!(self.reader_inbox.is_none());
        debug_assert!(self.reader_input_id.is_none());

        self.reader_inbox = Some(reader_inbox.clone());
        self.reader_input_id = Some(reader_input_id);

        BufferReader::Host(Box::new(ReaderD2H {
            buffer: None,
            outbound: self.inbound.clone(),
            inbound: self.outbound.clone(),
            item_size: self.item_size,
            writer_inbox: self.writer_inbox.clone(),
            writer_output_id: self.writer_output_id,
            my_inbox: reader_inbox,
            finished: false,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
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

unsafe impl Send for WriterD2H {}

#[derive(Debug)]
pub struct ReaderD2H {
    buffer: Option<CurrentBuffer>,
    inbound: Arc<Mutex<VecDeque<BufferFull>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty>>>,
    item_size: usize,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    my_inbox: Sender<BlockMessage>,
    finished: bool,
}

#[derive(Debug)]
struct CurrentBuffer {
    buffer: BufferFull,
    offset: usize,
}

#[async_trait]
impl BufferReaderHost for ReaderD2H {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        if self.buffer.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            } else {
                return (std::ptr::null::<u8>(), 0, Vec::new());
            }
        }

        unsafe {
            let buffer = self.buffer.as_ref().unwrap();
            let capacity = buffer.buffer.used_bytes / self.item_size;
            let ret = buffer.buffer.buffer.write().unwrap();
            (
                ret.as_ptr().add(buffer.offset * self.item_size),
                (capacity - buffer.offset) * self.item_size,
                Vec::new(),
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        debug_assert!(amount != 0);
        debug_assert!(self.buffer.is_some());

        let buffer = self.buffer.as_mut().unwrap();
        let capacity = buffer.buffer.used_bytes / self.item_size;

        debug_assert!(amount + buffer.offset <= capacity);

        buffer.offset += amount;
        if buffer.offset == capacity {
            let buffer = self.buffer.take().unwrap().buffer.buffer;
            self.outbound.lock().unwrap().push(BufferEmpty { buffer });
            let _ = self.writer_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again for another potentially
            // queued buffer. could also check if there is one and only
            // message in this case.
            let _ = self.my_inbox.try_send(BlockMessage::Notify);
        }
    }

    async fn notify_finished(&mut self) {
        debug!("D2H Reader finish");
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
        self.finished && self.inbound.lock().unwrap().is_empty()
    }
}

unsafe impl Send for ReaderD2H {}
