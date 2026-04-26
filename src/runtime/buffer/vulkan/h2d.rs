use ouroboros::self_referencing;
use std::any::Any;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::BufferWriteGuard;
use vulkano::buffer::Subbuffer;
use vulkano::buffer::subbuffer::BufferContents;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::vulkan::Buffer;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::ItemTag;

use super::d2h;

#[self_referencing]
#[derive(Debug)]
struct CurrentBuffer<T: BufferContents + CpuSample> {
    buffer: Subbuffer<[T]>,
    offset: usize,
    #[borrows(buffer)]
    #[covariant]
    guard: BufferWriteGuard<'this, [T]>,
}

// ====================== WRITER ============================
/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents + CpuSample> {
    current: Option<CurrentBuffer<T>>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter<T>>,
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct ConnectedWriter<T: BufferContents + CpuSample> {
    reader: PortEndpoint,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
}

impl<T> Writer<T>
where
    T: BufferContents + CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            current: None,
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            inbound: Arc::new(Mutex::new(Vec::new())),
            tags: Vec::new(),
        }
    }

    /// Add buffer to circuit
    pub fn add_buffer(&mut self, buffer: Buffer<T>) {
        self.inbound.lock().unwrap().push(buffer);
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut d2h::Reader<T>) {
        end.close_circuit(self.core.inbox(), self.inbound.clone());
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox) {
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
        let inbound = Arc::new(Mutex::new(Vec::new()));

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
        debug!("H2D writer called finish");

        if let Some(buffer) = self.current.take()
            && *buffer.borrow_offset() > 0
        {
            let offset = *buffer.borrow_offset();
            self.state
                .connected()
                .outbound
                .lock()
                .unwrap()
                .push(Buffer {
                    buffer: buffer.into_heads().buffer,
                    offset,
                });
        }

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

impl<T> CircuitWriter for Writer<T>
where
    T: BufferContents + CpuSample,
{
    type CircuitEnd = d2h::Reader<T>;

    fn close_circuit(&mut self, dst: &mut Self::CircuitEnd) {
        dst.close_circuit(self.core.inbox(), self.inbound.clone());
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: BufferContents + CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
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
            self.state
                .connected()
                .outbound
                .lock()
                .unwrap()
                .push(Buffer {
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

            self.state.connected().reader.inbox().notify();
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

// ====================== READER ============================
/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents + CpuSample> {
    core: PortCore,
    state: ConnectionState<ConnectedReader<T>>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader<T: BufferContents + CpuSample> {
    writer: PortEndpoint,
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
}

impl<T> Reader<T>
where
    T: BufferContents + CpuSample,
{
    /// Create a Reader
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            finished: false,
        }
    }

    /// Get full buffer
    pub fn buffers(&mut self) -> Vec<Buffer<T>> {
        let mut vec = self.state.connected().inbound.lock().unwrap();
        std::mem::take(&mut vec)
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    async fn notify_finished(&mut self) {
        debug!("H2D reader finish");
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
