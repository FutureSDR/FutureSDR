use std::any::Any;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
use xilinx_dma::DmaBuffer;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::zynq::BufferEmpty;
use crate::runtime::buffer::zynq::BufferFull;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::ItemTag;

#[derive(Debug)]
struct CurrentBuffer {
    buffer: DmaBuffer,
    byte_offset: usize,
}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter>,
    _p: PhantomData<D>,
}

#[derive(Debug)]
struct ConnectedWriter {
    reader: PortEndpoint,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            _p: PhantomData,
        }
    }

    /// All available empty buffers
    pub fn buffers(&mut self) -> Vec<BufferEmpty> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: BufferFull) {
        self.outbound.lock().unwrap().push_back(buffer);
        self.state.connected().reader.inbox().notify();
    }
}

impl<T> Default for Writer<T>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D> BufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Reader = Reader<D>;

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
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();

        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
        });

        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
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
pub struct Reader<D>
where
    D: CpuSample,
{
    current: Option<CurrentBuffer>,
    inbound: Arc<Mutex<VecDeque<BufferFull>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty>>>,
    core: PortCore,
    state: ConnectionState<ConnectedReader>,
    finished: bool,
    _p: PhantomData<D>,
}

#[derive(Debug)]
struct ConnectedReader {
    writer: PortEndpoint,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create Vulkan Device-to-Host Reader
    pub fn new() -> Self {
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            finished: false,
            _p: PhantomData,
        }
    }
}

impl<D> Default for Reader<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<D> BufferReader for Reader<D>
where
    D: CpuSample,
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

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        static V: Vec<ItemTag> = vec![];
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    byte_offset: 0,
                });
            } else {
                return (&[], &V);
            }
        }

        let current = self.current.as_mut().unwrap();

        unsafe {
            (
                std::slice::from_raw_parts(
                    (current.buffer.buffer() as *const u8).add(current.byte_offset) as *const D,
                    (current.buffer.size() - current.byte_offset) / size_of::<D>(),
                ),
                &V,
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        debug_assert!(self.current.is_some());

        let current = self.current.as_mut().unwrap();
        let byte_capacity = current.buffer.size() / size_of::<D>();

        debug_assert!(amount * size_of::<D>() + current.byte_offset <= byte_capacity);

        current.byte_offset += amount * size_of::<D>();
        if current.byte_offset == byte_capacity {
            let buffer = self.current.take().unwrap().buffer;
            self.outbound.lock().unwrap().push(BufferEmpty { buffer });
            self.state.connected().writer.inbox().notify();
            self.core.inbox().notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for zynq buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for zynq buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for zynq buffers");
        usize::MAX
    }
}
