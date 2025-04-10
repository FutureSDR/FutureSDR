use futures::prelude::*;
use ouroboros::self_referencing;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::BufferWriteGuard;
use vulkano::buffer::Subbuffer;

use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::buffer::vulkan::Buffer;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::Tags;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

use super::d2h;

#[self_referencing]
#[derive(Debug)]
struct CurrentBuffer<T: BufferContents> {
    buffer: Subbuffer<[T]>,
    offset: usize,
    #[borrows(buffer)]
    #[covariant]
    guard: BufferWriteGuard<'this, [T]>,
}

// ====================== WRITER ============================
/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents> {
    current: Option<CurrentBuffer<T>>,
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    inbox: Sender<BlockMessage>,
    block_id: BlockId,
    port_id: PortId,
    tags: Vec<ItemTag>,
    reader_inbox: Sender<BlockMessage>,
    reader_port_id: PortId,
}

impl<T> Writer<T>
where
    T: BufferContents,
{
    /// Create buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            inbox: rx.clone(),
            block_id: BlockId(0),
            port_id: PortId(String::new()),
            tags: Vec::new(),
            reader_inbox: rx,
            reader_port_id: PortId(String::new()),
        }
    }

    /// Add buffer to circuit
    pub fn add_buffer(&mut self, buffer: Buffer<T>) {
        self.inbound.lock().unwrap().push(buffer);
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut d2h::Reader<T>) {
        end.close_circuit(self.inbox.clone(), self.inbound.clone());
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
        self.reader_port_id = dest.port_id.clone();
        self.reader_inbox = dest.inbox.clone();
        dest.writer_inbox = self.inbox.clone();
        dest.writer_port_id = self.port_id.clone();
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");

        if let Some(buffer) = self.current.take() {
            if *buffer.borrow_offset() > 0 {
                let offset = *buffer.borrow_offset();
                self.outbound.lock().unwrap().push(Buffer {
                    buffer: buffer.into_heads().buffer,
                    offset,
                });
            }
        }

        let _ = self.reader_inbox
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

impl<T> CpuBufferWriter for Writer<T>
where
    T: BufferContents,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                let buffer = CurrentBufferBuilder {
                    offset: 0,
                    buffer: b.buffer,
                    guard_builder: |buffer| buffer.write().unwrap(),
                }
                .build();
                self.current = Some(buffer);
            } else {
                return &mut [];
            }
        }

        let b = self.current.as_mut().unwrap();
        let offset = *b.borrow_offset();
        &mut b.with_guard_mut(|guard| guard.deref_mut())[offset..]
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                let buffer = CurrentBufferBuilder {
                    offset: 0,
                    buffer: b.buffer,
                    guard_builder: |buffer| buffer.write().unwrap(),
                }
                .build();
                self.current = Some(buffer);
            } else {
                return (&mut [], Tags::new(&mut self.tags, 0));
            }
        }

        let b = self.current.as_mut().unwrap();
        let offset = *b.borrow_offset();
        let s = &mut b.with_guard_mut(|guard| guard.deref_mut())[offset..];
        (s, Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, n: usize) {
        let buffer = self.current.as_mut().unwrap();
        let offset = buffer.with_offset_mut(|offset| {
            *offset += n;
            *offset
        });
        let capacity = buffer.borrow_buffer().len();

        debug_assert!(offset as u64 <= capacity);
        if offset as u64 == capacity {
            let buffer = self.current.take().unwrap();
            self.outbound.lock().unwrap().push(Buffer {
                buffer: buffer.into_heads().buffer,
                offset,
            });

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                let buffer = CurrentBufferBuilder {
                    offset: 0,
                    buffer: b.buffer,
                    guard_builder: |buffer| buffer.write().unwrap(),
                }
                .build();
                self.current = Some(buffer);
            }

            let _ = self
                .reader_inbox
                .try_send(BlockMessage::Notify);
        }
    }
}

// ====================== READER ============================
/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents> {
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    inbox: Sender<BlockMessage>,
    block_id: BlockId,
    port_id: PortId,
    writer_port_id: PortId,
    writer_inbox: Sender<BlockMessage>,
    finished: bool,
}

impl<T> Reader<T>
where
    T: BufferContents,
{
    /// Create a Reader
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            inbound: Arc::new(Mutex::new(Vec::new())),
            inbox: rx.clone(),
            block_id: BlockId(0),
            port_id: PortId(String::new()),
            writer_port_id: PortId(String::new()),
            writer_inbox: rx,
            finished: false,
        }
    }

    /// Get full buffer
    pub fn buffers(&mut self) -> Vec<Buffer<T>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
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
        self.block_id = block_id;
        self.port_id = port_id;
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if !self.writer_inbox.is_closed() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    async fn notify_finished(&mut self) {
        debug!("H2D reader finish");
        if self.finished {
            return;
        }

        let _ = self.writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.port_id.clone(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&mut self) -> bool {
        self.finished
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}
