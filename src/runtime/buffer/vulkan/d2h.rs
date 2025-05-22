use futures::prelude::*;
use ouroboros::self_referencing;
use std::any::Any;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::BufferReadGuard;
use vulkano::buffer::Subbuffer;

use crate::channel::mpsc::channel;
use crate::channel::mpsc::Sender;
use crate::runtime::buffer::vulkan::Buffer;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

#[self_referencing]
#[derive(Debug)]
struct CurrentBuffer<T: BufferContents + CpuSample> {
    buffer: Subbuffer<[T]>,
    offset: usize,
    end: usize,
    #[borrows(buffer)]
    #[covariant]
    guard: BufferReadGuard<'this, [T]>,
}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents + CpuSample> {
    outbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    block_id: BlockId,
    port_id: PortId,
    inbox: Sender<BlockMessage>,
    reader_inbox: Sender<BlockMessage>,
    reader_port_id: PortId,
}

impl<T> Writer<T>
where
    T: BufferContents + CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            block_id: BlockId(0),
            port_id: PortId::default(),
            inbox: rx.clone(),
            reader_inbox: rx,
            reader_port_id: PortId::default(),
        }
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: Buffer<T>) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self.reader_inbox.try_send(BlockMessage::Notify);
    }
}

impl<T> Default for Writer<T>
where
    T: BufferContents + CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: BufferContents + CpuSample,
{
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if !self.reader_inbox.is_closed() {
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
        dest.writer_port_id = self.port_id.clone();
        dest.writer_inbox = self.inbox.clone();

        self.reader_inbox = dest.inbox.clone();
        self.reader_port_id = dest.port_id.clone();
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_port_id.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents + CpuSample> {
    current: Option<CurrentBuffer<T>>,
    inbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    block_id: BlockId,
    port_id: PortId,
    inbox: Sender<BlockMessage>,
    writer_inbox: Sender<BlockMessage>,
    circuit_start_inbox: Sender<BlockMessage>,
    writer_port_id: PortId,
    tags: Vec<ItemTag>,
    finished: bool,
}
impl<T> Reader<T>
where
    T: BufferContents + CpuSample,
{
    /// Create Vulkan Device-to-Host Reader
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            block_id: BlockId(0),
            port_id: PortId::default(),
            inbox: rx.clone(),
            writer_inbox: rx.clone(),
            circuit_start_inbox: rx,
            writer_port_id: PortId::default(),
            tags: Vec::new(),
            finished: false,
        }
    }

    /// Close Circuit
    pub fn close_circuit(
        &mut self,
        circuit_start_inbox: Sender<BlockMessage>,
        outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    ) {
        self.circuit_start_inbox = circuit_start_inbox;
        self.outbound = outbound;
    }
}

impl<T> Default for Reader<T>
where
    T: BufferContents + CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> BufferReader for Reader<T>
where
    T: BufferContents + CpuSample,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if !self.writer_inbox.is_closed() && !self.circuit_start_inbox.is_closed() {
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

        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_port_id.clone(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: BufferContents + CpuSample,
{
    type Item = T;

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

            let _ = self.circuit_start_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again for another potentially
            // queued buffer. could also check if there is one and only
            // message in this case.
            let _ = self.inbox.try_send(BlockMessage::Notify);
        }
    }
}
