use bytemuck::Pod;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;
use wgpu::BufferView;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::buffer::ThreadSafeConnect;
use crate::runtime::buffer::wgpu::OutputBufferEmpty as BufferEmpty;
use crate::runtime::buffer::wgpu::OutputBufferFull as BufferFull;
use crate::runtime::dev::ItemTag;

#[derive(Debug)]
struct CurrentBuffer<D>
where
    D: CpuSample,
{
    buffer: BufferFull<D>,
    byte_offset: usize,
    slice: BufferView,
}

/// WGPU device-to-host writer that accepts full readback buffers.
#[derive(Debug)]
pub struct Writer<D: CpuSample> {
    inbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    instance: Option<super::Instance>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter>,
    max_contiguous_items: Option<usize>,
}

#[derive(Debug)]
struct ConnectedWriter {
    reader: PortEndpoint,
}

/// Reader offer for a native WGPU D2H cross-domain connection.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub struct ThreadSafeConnectToken<D>
where
    D: CpuSample,
{
    reader: PortEndpoint,
    _item: PhantomData<D>,
}

/// Reader installation returned by the native WGPU D2H writer.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub struct ThreadSafeReturnToken<D>
where
    D: CpuSample,
{
    inbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    instance: Option<super::Instance>,
    connected: ConnectedReader,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create a WGPU device-to-host writer.
    pub fn new() -> Self {
        Writer {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            instance: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            max_contiguous_items: None,
        }
    }

    /// Take all empty readback buffers available for device output.
    ///
    /// The returned tokens are explicit WGPU handoff resources, not in-place
    /// circuit buffers with drop-based recycling.
    pub fn buffers(&mut self) -> Vec<BufferEmpty<D>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Set WGPU instance used to allocate reusable readback buffers.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
    }

    /// Inject reusable output readback buffers.
    pub fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let Some(instance) = self.instance.as_ref() else {
            panic!("D2H writer: set_instance() must be called before injecting buffers");
        };
        if n_buffers > 0 {
            assert!(n_items > 0, "D2H buffers cannot be empty");
        }
        let n_bytes = (n_items * D::SIZE.get()) as u64;
        let mut inbound = self.inbound.lock().unwrap();
        for _ in 0..n_buffers {
            inbound.push(BufferEmpty {
                buffer: instance.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("d2h_output_buffer"),
                    size: n_bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                _p: PhantomData,
            });
        }
        if n_buffers > 0 {
            self.max_contiguous_items = Some(
                self.max_contiguous_items
                    .map_or(n_items, |current| current.min(n_items)),
            );
        }
    }

    /// Submit a full readback buffer to the downstream CPU reader.
    ///
    /// This is the explicit handoff path from the accelerator block to the CPU
    /// reader; it is not an in-place circuit close/recycle operation.
    pub fn submit(&mut self, buffer: BufferFull<D>) {
        self.outbound.lock().unwrap().push_back(buffer);
        self.state.connected().reader.inbox().notify();
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

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.instance.is_none() {
            Err(Error::ValidationError(
                "D2H writer: no wgpu instance configured".to_string(),
            ))
        } else if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();
        dest.instance = self.instance.clone();

        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox().clone(), dest.core.port_id()),
        });

        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
            max_contiguous_items: self.max_contiguous_items,
        });
    }

    async fn notify_finished(&mut self) {
        let reader = &self.state.connected().reader;
        let _ = reader
            .inbox()
            .send(BlockMessage::StreamInputDone {
                input_id: reader.port_id(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<D> ThreadSafeConnect for Writer<D>
where
    D: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<D>;
    type WriterToken = ThreadSafeReturnToken<D>;

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
            instance: self.instance.clone(),
            connected: ConnectedReader {
                writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
                max_contiguous_items: self.max_contiguous_items,
            },
        }
    }

    fn finish_reader(reader: &mut Reader<D>, token: Self::WriterToken) {
        reader.inbound = token.outbound;
        reader.outbound = token.inbound;
        reader.instance = token.instance;
        reader.state.set_connected(token.connected);
    }
}

/// WGPU device-to-host CPU reader.
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
    buffer: Option<CurrentBuffer<D>>,
    inbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    instance: Option<super::Instance>,
    core: PortCore,
    state: ConnectionState<ConnectedReader>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader {
    writer: PortEndpoint,
    max_contiguous_items: Option<usize>,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create a WGPU device-to-host reader.
    pub fn new() -> Self {
        Self {
            buffer: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            instance: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
        }
    }

    /// Set WGPU instance.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
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

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.instance.is_none() {
            Err(Error::ValidationError(
                "D2H reader: no wgpu instance configured".to_string(),
            ))
        } else if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let writer = &self.state.connected().writer;
        let _ = writer
            .inbox()
            .send(BlockMessage::StreamOutputDone {
                output_id: writer.port_id(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished && self.inbound.lock().unwrap().is_empty()
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample + Pod,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        if self.buffer.is_none() {
            if let Some(buffer) = self.inbound.lock().unwrap().pop_front() {
                let slice = buffer
                    .buffer
                    .slice(0..buffer.used_bytes as u64)
                    .get_mapped_range();
                self.buffer = Some(CurrentBuffer {
                    buffer,
                    byte_offset: 0,
                    slice,
                });
            } else {
                return (&[], &[]);
            }
        }

        let buffer = self.buffer.as_ref().unwrap();
        let data = bytemuck::try_cast_slice(&buffer.slice[buffer.byte_offset..])
            .expect("D2H reader: mapped buffer alignment invalid for sample type");
        (data, &[])
    }

    fn consume(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        debug_assert!(self.buffer.is_some());

        let buffer = self.buffer.as_mut().unwrap();
        let byte_len = buffer.slice.len();
        debug_assert!(amount * D::SIZE.get() + buffer.byte_offset <= byte_len);

        buffer.byte_offset += amount * D::SIZE.get();
        if buffer.byte_offset == byte_len {
            let CurrentBuffer { buffer, slice, .. } = self.buffer.take().unwrap();
            drop(slice);
            let buffer = buffer.buffer;
            buffer.unmap();
            self.outbound.lock().unwrap().push(BufferEmpty {
                buffer,
                _p: PhantomData,
            });
            self.state.connected().writer.inbox().notify();
            self.core.inbox().notify();
        }
    }

    fn max_contiguous_items(&self) -> usize {
        self.buffer
            .as_ref()
            .map(|buffer| (buffer.slice.len() - buffer.byte_offset) / D::SIZE.get())
            .or(self.state.connected().max_contiguous_items)
            .expect("D2H buffer capacity queried without a current page")
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::Reader;
    use super::Writer;

    fn assert_send<T: Send>() {}

    #[test]
    fn d2h_buffers_are_send_by_auto_traits() {
        assert_send::<Writer<f32>>();
        assert_send::<Reader<f32>>();
    }
}
