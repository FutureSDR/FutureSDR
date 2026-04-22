use ouroboros::self_referencing;
use std::any::Any;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::BufferReadGuard;
use vulkano::buffer::Subbuffer;
use vulkano::buffer::subbuffer::BufferContents;

use crate::runtime::BlockId;
use crate::runtime::BlockInbox;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitReturn;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::vulkan::Buffer;

type ReturnQueue<T> = Arc<Mutex<Vec<Buffer<T>>>>;

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
    core: PortCore,
    state: ConnectionState<ConnectedWriter<T>>,
}

#[derive(Debug)]
struct ConnectedWriter<T: BufferContents + CpuSample> {
    reader: PortEndpoint,
    outbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
}

impl<T> Writer<T>
where
    T: BufferContents + CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
        }
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: Buffer<T>) {
        self.state
            .connected()
            .outbound
            .lock()
            .unwrap()
            .push_back(buffer);
        self.state.connected().reader.inbox().notify();
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        let inbound = Arc::new(Mutex::new(VecDeque::new()));

        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
            outbound: inbound.clone(),
        });

        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
            inbound,
        });
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .state
            .connected()
            .reader
            .inbox()
            .send(BlockMessage::StreamInputDone {
                input_id: self.state.connected().reader.port_id(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents + CpuSample> {
    current: Option<CurrentBuffer<T>>,
    core: PortCore,
    state: ConnectionState<ConnectedReader<T>>,
    circuit_start: Option<CircuitReturn<ReturnQueue<T>>>,
    tags: Vec<ItemTag>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader<T: BufferContents + CpuSample> {
    writer: PortEndpoint,
    inbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
}

impl<T> Reader<T>
where
    T: BufferContents + CpuSample,
{
    /// Create Vulkan Device-to-Host Reader
    pub fn new() -> Self {
        Self {
            current: None,
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            circuit_start: None,
            tags: Vec::new(),
            finished: false,
        }
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, circuit_start_inbox: BlockInbox, outbound: ReturnQueue<T>) {
        self.circuit_start = Some(CircuitReturn::new(circuit_start_inbox.notifier(), outbound));
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.state.is_connected() && self.circuit_start.is_some() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let _ = self
            .state
            .connected()
            .writer
            .inbox()
            .send(BlockMessage::StreamOutputDone {
                output_id: self.state.connected().writer.port_id(),
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
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: BufferContents + CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            if let Some(b) = self.state.connected().inbound.lock().unwrap().pop_front() {
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
            self.circuit_start
                .as_ref()
                .unwrap()
                .queue()
                .lock()
                .unwrap()
                .push(Buffer {
                    buffer: buffer.into_heads().buffer,
                    offset: 0,
                });

            self.circuit_start.as_ref().unwrap().notify();
            self.core.inbox().notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for Vulkan buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for Vulkan buffers");
    }

    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for Vulkan buffers");
        usize::MAX
    }
}
