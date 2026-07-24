use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;
use xilinx_dma::DmaBuffer;

use crate::buffer::BufferEmpty;
use crate::buffer::BufferFull;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BlockInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::ConnectionState;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::PortEndpoint;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::tracing::debug;
use futuresdr::tracing::warn;

#[derive(Debug)]
struct CurrentBuffer {
    buffer: DmaBuffer,
    byte_offset: usize,
}

// ====================== WRITER ============================
/// Zynq host-to-device CPU writer.
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    current: Option<CurrentBuffer>,
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter>,
    tags: Vec<ItemTag>,
    _p: PhantomData<D>,
}

#[derive(Debug)]
struct ConnectedWriter {
    reader: PortEndpoint,
}

pub struct ThreadSafeConnectToken<D>
where
    D: CpuSample,
{
    reader: PortEndpoint,
    _item: PhantomData<D>,
}

pub struct ThreadSafeReturnToken {
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    connected: ConnectedReader,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create a Zynq host-to-device writer.
    pub fn new() -> Self {
        debug!("H2D writer created");
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            tags: Vec::new(),
            _p: PhantomData,
        }
    }
}

impl<D> Default for Writer<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D> BufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Inbox = BlockInbox;
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
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
            reader: PortEndpoint::new(dest.core.inbox().clone(), dest.core.port_id()),
        });

        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
        });
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");

        if let Some(CurrentBuffer {
            byte_offset,
            buffer,
        }) = self.current.take()
            && byte_offset > 0
        {
            self.outbound.lock().unwrap().push_back(BufferFull {
                buffer,
                used_bytes: byte_offset,
            });
        }

        let reader = &self.state.connected().reader;
        let _ = reader.inbox().stream_input_done(reader.port_id()).await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D> ThreadSafeConnect for Writer<D>
where
    D: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<D>;
    type WriterToken = ThreadSafeReturnToken;

    fn take_reader_token(reader: &mut Reader<D>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox().clone(), reader.core.port_id()),
            _item: PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        self.state.set_connected(ConnectedWriter {
            reader: token.reader,
        });
        ThreadSafeReturnToken {
            inbound: self.inbound.clone(),
            outbound: self.outbound.clone(),
            connected: ConnectedReader {
                writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
            },
        }
    }

    fn finish_reader(reader: &mut Reader<D>, token: Self::WriterToken) {
        reader.inbound = token.outbound;
        reader.outbound = token.inbound;
        reader.state.set_connected(token.connected);
    }
}

impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample + bytemuck::Pod,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    byte_offset: 0,
                });
            } else {
                return (&mut [], Tags::new(&mut self.tags, 0));
            }
        }

        let current = self.current.as_mut().unwrap();

        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                (current.buffer.buffer() as *mut u8).add(current.byte_offset),
                current.buffer.size() - current.byte_offset,
            )
        };
        let samples = bytemuck::try_cast_slice_mut(bytes)
            .expect("Zynq H2D buffer alignment invalid for sample type");
        (samples, Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, n: usize) {
        let current = self.current.as_mut().unwrap();
        let byte_capacity = current.buffer.size();

        debug_assert!(n * D::SIZE.get() + current.byte_offset <= byte_capacity);
        current.byte_offset += n * D::SIZE.get();
        if current.byte_offset == byte_capacity {
            let buffer = self.current.take().unwrap().buffer;
            self.outbound.lock().unwrap().push_back(BufferFull {
                buffer,
                used_bytes: byte_capacity,
            });

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    byte_offset: 0,
                });
            }

            self.state.connected().reader.inbox().notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for Zynq buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for Zynq buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for Zynq buffers");
        usize::MAX
    }
}

// ====================== READER ============================
/// Zynq host-to-device reader that exposes full DMA buffers.
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
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
    /// Create a Zynq host-to-device reader.
    pub fn new() -> Self {
        Self {
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
            _p: PhantomData,
        }
    }

    /// Return an empty DMA buffer to the writer.
    ///
    /// This is the explicit reusable-resource return path for Zynq H2D
    /// handoff buffers. These tokens are not in-place circuit buffers.
    pub fn submit(&mut self, buffer: BufferEmpty) {
        self.outbound.lock().unwrap().push(buffer);
        self.state.connected().writer.inbox().notify();
    }

    /// Get the next full DMA buffer, if available.
    ///
    /// The returned token must be handled by the accelerator block and returned
    /// explicitly; it does not carry an in-place circuit drop-recycle handle.
    pub fn get_buffer(&mut self) -> Option<BufferFull> {
        let mut vec = self.inbound.lock().unwrap();
        vec.pop_front()
    }

    /// Check whether a full buffer is available.
    pub fn buffer_available(&self) -> bool {
        let vec = self.inbound.lock().unwrap();
        !vec.is_empty()
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

impl<D> BufferReader for Reader<D>
where
    D: CpuSample,
{
    type Inbox = BlockInbox;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
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
        let writer = &self.state.connected().writer;
        let _ = writer.inbox().stream_output_done(writer.port_id()).await;
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

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}
