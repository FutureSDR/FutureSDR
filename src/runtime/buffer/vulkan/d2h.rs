use futures::prelude::*;
use ouroboros::self_referencing;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::BufferReadGuard;
use vulkano::buffer::Subbuffer;

use crate::channel::mpsc::Sender;
use crate::runtime::buffer::vulkan::Buffer;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

#[self_referencing]
#[derive(Debug)]
struct CurrentBuffer<T: BufferContents> {
    buffer: Subbuffer<[T]>,
    offset: usize,
    end: usize,
    #[borrows(buffer)]
    #[covariant]
    guard: BufferReadGuard<'this, [T]>,
}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents> {
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    outbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    inbox: Option<Sender<BlockMessage>>,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_port_id: Option<PortId>,
}

impl<T> Writer<T>
where
    T: BufferContents,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            block_id: None,
            port_id: None,
            inbox: None,
            reader_inbox: None,
            reader_port_id: None,
        }
    }

    /// All available empty buffers
    pub fn buffers(&mut self) -> Vec<Buffer<T>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: Buffer<T>) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self
            .reader_inbox
            .as_mut()
            .unwrap()
            .try_send(BlockMessage::Notify);
    }
}

impl<T> Default for Writer<T>
where
    T: BufferContents,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: BufferContents,
{
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.reader_inbox.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();
        dest.writer_port_id = self.port_id.clone();
        dest.writer_inbox = self.inbox.clone();

        self.reader_inbox = dest.inbox.clone();
        self.reader_port_id = dest.port_id.clone();
    }

    async fn notify_finished(&mut self) {
        self.reader_inbox
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_port_id.clone().unwrap(),
            })
            .await
            .unwrap();
    }

    fn block_id(&self) -> BlockId {
        self.block_id.unwrap()
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone().unwrap()
    }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents> {
    current: Option<CurrentBuffer<T>>,
    inbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    inbox: Option<Sender<BlockMessage>>,
    writer_inbox: Option<Sender<BlockMessage>>,
    writer_port_id: Option<PortId>,
    tags: Vec<ItemTag>,
    finished: bool,
}
impl<T> Reader<T>
where
    T: BufferContents,
{
    /// Create Vulkan Device-to-Host Reader
    pub fn new() -> Self {
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            block_id: None,
            port_id: None,
            inbox: None,
            writer_inbox: None,
            writer_port_id: None,
            tags: Vec::new(),
            finished: false,
        }
    }
}

impl<T> Default for Reader<T>
where
    T: BufferContents,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BufferReader for Reader<T>
where
    T: BufferContents,
{
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer_inbox.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        self.writer_inbox
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_port_id.clone().unwrap(),
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&mut self) -> bool {
        self.finished
    }

    fn block_id(&self) -> BlockId {
        self.block_id.unwrap()
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone().unwrap()
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: BufferContents,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                let buffer = CurrentBufferBuilder {
                    buffer: b.buffer,
                    offset: 0,
                    end: b.offset,
                    guard_builder: |buffer| buffer.read().unwrap(),
                }
                .build();
                self.current = Some(buffer);
            } else {
                return &[];
            }
        }

        let current = self.current.as_ref().unwrap();
        let offset = *current.borrow_offset();
        let end = *current.borrow_end();
        &current.with_guard(|guard| guard.deref())[offset..end]
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                let buffer = CurrentBufferBuilder {
                    buffer: b.buffer,
                    offset: 0,
                    end: b.offset,
                    guard_builder: |buffer| buffer.read().unwrap(),
                }
                .build();
                self.current = Some(buffer);
            } else {
                return (&[], &self.tags);
            }
        }

        let current = self.current.as_ref().unwrap();
        let offset = *current.borrow_offset();
        let end = *current.borrow_end();
        let s = &current.with_guard(|guard| guard.deref())[offset..end];
        (s, &self.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        debug!("consuming {}", n);
        let buffer = self.current.as_mut().unwrap();
        let offset = buffer.with_offset_mut(|offset| {
            *offset += n;
            *offset
        });

        let capacity = *buffer.borrow_end();
        debug_assert!(offset <= capacity);

        if offset == capacity {
            let buffer = self.current.take().unwrap();
            self.outbound.lock().unwrap().push(Buffer {
                buffer: buffer.into_heads().buffer,
                offset: 0,
            });

            let _ = self
                .writer_inbox
                .as_mut()
                .unwrap()
                .try_send(BlockMessage::Notify);

            // make sure to be called again for another potentially
            // queued buffer. could also check if there is one and only
            // message in this case.
            let _ = self.inbox.as_mut().unwrap().try_send(BlockMessage::Notify);
        }
    }
}
