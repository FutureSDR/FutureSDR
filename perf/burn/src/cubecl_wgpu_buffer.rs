use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

use bytemuck::Pod;
use cubecl::Runtime as _;
use cubecl::client::ComputeClient;
use cubecl::server::Handle;
use cubecl::wgpu::AutoGraphicsApi;
use cubecl::wgpu::RuntimeOptions;
use cubecl::wgpu::WgpuDevice;
use cubecl::wgpu::WgpuResource;
use cubecl::wgpu::WgpuRuntime;
use cubecl::wgpu::WgpuSetup;
use cubecl::wgpu::init_setup;
use cubecl_runtime::storage::ManagedResource;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BlockInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::ConnectionState;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::PortEndpoint;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::dev::ItemTag;
use tracing::debug;
use tracing::warn;
use wgpu::BufferUsages;
use wgpu::BufferView;
use wgpu::BufferViewMut;

#[derive(Clone)]
pub struct CubeWgpuContext {
    pub device: WgpuDevice,
    pub setup: WgpuSetup,
    pub client: ComputeClient<WgpuRuntime>,
}

impl CubeWgpuContext {
    pub fn new() -> Self {
        let device = WgpuDevice::default();
        let setup = init_setup::<AutoGraphicsApi>(&device, RuntimeOptions::default());
        let client = WgpuRuntime::client(&device);
        Self {
            device,
            setup,
            client,
        }
    }
}

impl Default for CubeWgpuContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CubeBufferResource {
    _guard: ManagedResource<WgpuResource>,
    pub buffer: wgpu::Buffer,
    pub offset: u64,
    pub size: u64,
}

impl CubeBufferResource {
    pub fn new(context: &CubeWgpuContext, handle: Handle) -> anyhow::Result<Self> {
        let guard = context
            .client
            .get_resource(handle)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let resource = guard.resource();
        Ok(Self {
            buffer: resource.buffer.clone(),
            offset: resource.offset,
            size: resource.size,
            _guard: guard,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadSlotState {
    WritableMapped,
    ReadyForGpu,
    InUse,
    Remapping,
}

struct UploadSlot<D: CpuSample> {
    staging: wgpu::Buffer,
    capacity: usize,
    written_items: usize,
    state: UploadSlotState,
    handle: Handle,
    resource: CubeBufferResource,
    _p: PhantomData<D>,
}

#[derive(Debug)]
struct CurrentUploadSlot {
    slot_id: usize,
    item_offset: usize,
    view: BufferViewMut,
}

pub struct InputBufferFull<D: CpuSample> {
    pub handle: Handle,
    pub n_items: usize,
    pub capacity: usize,
    pub slot_id: usize,
    _p: PhantomData<D>,
}

pub struct InputBufferEmpty<D: CpuSample> {
    pub slot_id: usize,
    pub capacity: usize,
    _p: PhantomData<D>,
}

impl<D: CpuSample> InputBufferFull<D> {
    pub fn into_empty(self) -> InputBufferEmpty<D> {
        InputBufferEmpty {
            slot_id: self.slot_id,
            capacity: self.capacity,
            _p: PhantomData,
        }
    }
}

pub struct H2DWriter<D: CpuSample> {
    current: Option<CurrentUploadSlot>,
    slots: Arc<Mutex<Vec<UploadSlot<D>>>>,
    writable_ids: Arc<Mutex<Vec<usize>>>,
    ready_ids: Arc<Mutex<VecDeque<usize>>>,
    context: Option<CubeWgpuContext>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter>,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct ConnectedWriter {
    reader: PortEndpoint,
}

pub struct H2DThreadSafeConnectToken<D: CpuSample> {
    reader: PortEndpoint,
    context: Option<CubeWgpuContext>,
    _item: PhantomData<D>,
}

pub struct H2DThreadSafeReturnToken<D: CpuSample> {
    slots: Arc<Mutex<Vec<UploadSlot<D>>>>,
    writable_ids: Arc<Mutex<Vec<usize>>>,
    ready_ids: Arc<Mutex<VecDeque<usize>>>,
    context: Option<CubeWgpuContext>,
    connected: ConnectedReader,
}

impl<D: CpuSample> H2DWriter<D> {
    pub fn new() -> Self {
        Self {
            current: None,
            slots: Arc::new(Mutex::new(Vec::new())),
            writable_ids: Arc::new(Mutex::new(Vec::new())),
            ready_ids: Arc::new(Mutex::new(VecDeque::new())),
            context: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            tags: Vec::new(),
        }
    }

    pub fn set_context(&mut self, context: CubeWgpuContext) {
        self.context = Some(context);
    }

    pub fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let Some(context) = self.context.as_ref() else {
            panic!("CubeCL H2D writer: set_context() must be called before injecting buffers");
        };
        let n_bytes = n_items * D::SIZE.get();
        assert_eq!(
            n_bytes % wgpu::COPY_BUFFER_ALIGNMENT as usize,
            0,
            "CubeCL H2D writer: item capacity must be 4-byte aligned"
        );

        let mut slots = self.slots.lock().unwrap();
        let mut writable_ids = self.writable_ids.lock().unwrap();
        for _ in 0..n_buffers {
            let slot_id = slots.len();
            let handle = context.client.empty(n_bytes);
            let resource =
                CubeBufferResource::new(context, handle.clone()).expect("CubeCL H2D resource");
            assert!(
                resource.size >= n_bytes as u64,
                "CubeCL H2D resource is smaller than requested buffer"
            );
            slots.push(UploadSlot {
                staging: context.setup.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cubecl_h2d_staging_buffer"),
                    size: n_bytes as u64,
                    usage: BufferUsages::MAP_WRITE | BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                capacity: n_items,
                written_items: 0,
                state: UploadSlotState::WritableMapped,
                handle,
                resource,
                _p: PhantomData,
            });
            writable_ids.push(slot_id);
        }
    }

    fn finalize_current(&mut self, used_items: usize) {
        let current = self.current.take().unwrap();
        let slot_id = current.slot_id;
        drop(current.view);

        let (staging, dst, dst_offset, n_bytes) = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots
                .get_mut(slot_id)
                .expect("CubeCL H2D writer: invalid slot id");
            assert_eq!(
                slot.state,
                UploadSlotState::WritableMapped,
                "CubeCL H2D writer: finalize on non-writable slot"
            );
            slot.written_items = used_items;
            slot.state = UploadSlotState::ReadyForGpu;
            slot.staging.unmap();
            (
                slot.staging.clone(),
                slot.resource.buffer.clone(),
                slot.resource.offset,
                used_items * D::SIZE.get(),
            )
        };

        let context = self
            .context
            .as_ref()
            .expect("CubeCL H2D writer: context missing");
        if n_bytes > 0 {
            let mut encoder =
                context
                    .setup
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("cubecl_h2d_copy_encoder"),
                    });
            encoder.copy_buffer_to_buffer(&staging, 0, &dst, dst_offset, n_bytes as u64);
            context.setup.queue.submit(Some(encoder.finish()));
        }

        self.ready_ids.lock().unwrap().push_back(slot_id);
    }

    fn acquire_current(&mut self) -> Option<()> {
        if self.current.is_some() {
            return Some(());
        }
        let slot_id = self.writable_ids.lock().unwrap().pop()?;
        let (capacity, view) = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots
                .get_mut(slot_id)
                .expect("CubeCL H2D writer: invalid slot id");
            assert_eq!(
                slot.state,
                UploadSlotState::WritableMapped,
                "CubeCL H2D writer: acquired non-writable slot"
            );
            slot.written_items = 0;
            let byte_len = (slot.capacity * D::SIZE.get()) as u64;
            (
                slot.capacity,
                slot.staging.slice(0..byte_len).get_mapped_range_mut(),
            )
        };
        self.current = Some(CurrentUploadSlot {
            slot_id,
            item_offset: 0,
            view,
        });
        debug_assert!(capacity > 0);
        Some(())
    }
}

impl<D: CpuSample> Default for H2DWriter<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: CpuSample> BufferWriter for H2DWriter<D> {
    type Inbox = BlockInbox;
    type Reader = H2DReader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.context.is_none() {
            Err(Error::ValidationError(
                "CubeCL H2D writer: no context configured".to_string(),
            ))
        } else if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        if self.context.is_none() {
            self.context = dest.context.clone();
        }
        dest.slots = self.slots.clone();
        dest.ready_ids = self.ready_ids.clone();
        dest.writable_ids = self.writable_ids.clone();
        dest.context = self.context.clone();

        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
        });
        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
        });
    }

    async fn notify_finished(&mut self) {
        if let Some(current) = self.current.as_ref() {
            if current.item_offset > 0 {
                self.finalize_current(current.item_offset);
                self.state.connected().reader.inbox().notify();
            } else {
                let current = self.current.take().unwrap();
                let slot_id = current.slot_id;
                drop(current.view);
                {
                    let mut slots = self.slots.lock().unwrap();
                    let slot = slots
                        .get_mut(slot_id)
                        .expect("CubeCL H2D writer: invalid slot id");
                    slot.written_items = 0;
                    slot.state = UploadSlotState::WritableMapped;
                }
                self.writable_ids.lock().unwrap().push(slot_id);
            }
        }

        let reader = self.state.connected().reader.clone();
        let _ = reader.inbox().stream_input_done(reader.port_id()).await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D: CpuSample> ThreadSafeConnect for H2DWriter<D> {
    type ReaderToken = H2DThreadSafeConnectToken<D>;
    type WriterToken = H2DThreadSafeReturnToken<D>;

    fn take_reader_token(reader: &mut H2DReader<D>) -> Self::ReaderToken {
        H2DThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox(), reader.core.port_id()),
            context: reader.context.clone(),
            _item: PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        if self.context.is_none() {
            self.context = token.context;
        }
        self.state.set_connected(ConnectedWriter {
            reader: token.reader,
        });
        H2DThreadSafeReturnToken {
            slots: self.slots.clone(),
            writable_ids: self.writable_ids.clone(),
            ready_ids: self.ready_ids.clone(),
            context: self.context.clone(),
            connected: ConnectedReader {
                writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
            },
        }
    }

    fn finish_reader(reader: &mut H2DReader<D>, token: Self::WriterToken) {
        reader.slots = token.slots;
        reader.ready_ids = token.ready_ids;
        reader.writable_ids = token.writable_ids;
        reader.context = token.context;
        reader.state.set_connected(token.connected);
    }
}

impl<D: CpuSample + Pod> CpuBufferWriter for H2DWriter<D> {
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.acquire_current().is_none() {
            return (&mut [], Tags::new(&mut self.tags, 0));
        }

        let current = self.current.as_mut().unwrap();
        let cap = {
            let slots = self.slots.lock().unwrap();
            slots[current.slot_id].capacity
        };
        let byte_offset = current.item_offset * D::SIZE.get();
        let byte_end = cap * D::SIZE.get();
        let mut tail_write_only = current.view.slice(byte_offset..byte_end);
        let tail = unsafe {
            std::slice::from_raw_parts_mut(
                tail_write_only.as_raw_element_ptr().as_ptr(),
                byte_end - byte_offset,
            )
        };
        let data = bytemuck::try_cast_slice_mut(tail)
            .expect("CubeCL H2D writer: mapped buffer alignment invalid");
        (data, Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        let current = self.current.as_mut().unwrap();
        let item_capacity = {
            let slots = self.slots.lock().unwrap();
            slots[current.slot_id].capacity
        };
        assert!(
            amount + current.item_offset <= item_capacity,
            "CubeCL H2D writer overflow: produce {} at offset {} exceeds capacity {}",
            amount,
            current.item_offset,
            item_capacity
        );
        current.item_offset += amount;
        if current.item_offset == item_capacity {
            self.finalize_current(item_capacity);
            self.state.connected().reader.inbox().notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items is not implemented for CubeCL H2D buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items is not implemented for CubeCL H2D buffers");
    }

    fn max_items(&self) -> usize {
        usize::MAX
    }
}

pub struct H2DReader<D: CpuSample> {
    slots: Arc<Mutex<Vec<UploadSlot<D>>>>,
    ready_ids: Arc<Mutex<VecDeque<usize>>>,
    writable_ids: Arc<Mutex<Vec<usize>>>,
    context: Option<CubeWgpuContext>,
    core: PortCore,
    state: ConnectionState<ConnectedReader>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader {
    writer: PortEndpoint,
}

impl<D: CpuSample> H2DReader<D> {
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
            ready_ids: Arc::new(Mutex::new(VecDeque::new())),
            writable_ids: Arc::new(Mutex::new(Vec::new())),
            context: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
        }
    }

    pub fn set_context(&mut self, context: CubeWgpuContext) {
        self.context = Some(context);
    }

    pub fn submit(&mut self, buffer: InputBufferEmpty<D>) {
        let Some(context) = self.context.clone() else {
            panic!("CubeCL H2D reader: set_context() must be called before submit");
        };
        let slot_id = buffer.slot_id;
        let (staging, capacity) = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots
                .get_mut(slot_id)
                .expect("CubeCL H2D reader: invalid slot id");
            assert_eq!(
                slot.state,
                UploadSlotState::InUse,
                "CubeCL H2D reader: submit on non-in-use slot"
            );
            if slot.capacity != buffer.capacity {
                warn!(
                    "CubeCL H2D reader: capacity mismatch on submit (slot {} has {}, submit has {})",
                    slot_id, slot.capacity, buffer.capacity
                );
            }
            slot.state = UploadSlotState::Remapping;
            (slot.staging.clone(), slot.capacity)
        };

        let writable_ids = self.writable_ids.clone();
        let slots_arc = self.slots.clone();
        let writer_inbox = self.state.connected().writer.inbox();
        let byte_len = (capacity * D::SIZE.get()) as u64;
        let slice = staging.slice(0..byte_len);
        slice.map_async(wgpu::MapMode::Write, move |result| match result {
            Ok(()) => {
                {
                    let mut slots = slots_arc.lock().unwrap();
                    let slot = slots
                        .get_mut(slot_id)
                        .expect("CubeCL H2D reader: invalid slot id in map callback");
                    slot.written_items = 0;
                    slot.state = UploadSlotState::WritableMapped;
                }
                writable_ids.lock().unwrap().push(slot_id);
                writer_inbox.notify();
            }
            Err(e) => warn!(
                "CubeCL H2D reader: map_async(write) failed for slot {}: {:?}",
                slot_id, e
            ),
        });
        let _ = context.setup.device.poll(wgpu::PollType::Poll);
    }

    pub fn get_buffer(&mut self) -> Option<InputBufferFull<D>> {
        let slot_id = self.ready_ids.lock().unwrap().pop_front()?;
        let mut slots = self.slots.lock().unwrap();
        let slot = slots
            .get_mut(slot_id)
            .expect("CubeCL H2D reader: invalid slot id");
        assert_eq!(
            slot.state,
            UploadSlotState::ReadyForGpu,
            "CubeCL H2D reader: get_buffer on non-ready slot"
        );
        slot.state = UploadSlotState::InUse;
        Some(InputBufferFull {
            handle: slot.handle.clone(),
            n_items: slot.written_items,
            capacity: slot.capacity,
            slot_id,
            _p: PhantomData,
        })
    }
}

impl<D: CpuSample> Default for H2DReader<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: CpuSample> BufferReader for H2DReader<D> {
    type Inbox = BlockInbox;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.context.is_none() {
            Err(Error::ValidationError(
                "CubeCL H2D reader: no context configured".to_string(),
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
        let writer = self.state.connected().writer.clone();
        let _ = writer.inbox().stream_output_done(writer.port_id()).await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished && self.ready_ids.lock().unwrap().is_empty()
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

pub struct OutputBufferEmpty<D: CpuSample> {
    pub buffer: wgpu::Buffer,
    pub capacity: usize,
    _p: PhantomData<D>,
}

pub struct OutputBufferFull<D: CpuSample> {
    pub buffer: wgpu::Buffer,
    pub used_bytes: usize,
    _p: PhantomData<D>,
}

pub struct D2HWriter<D: CpuSample> {
    inbound: Arc<Mutex<Vec<OutputBufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<OutputBufferFull<D>>>>,
    context: Option<CubeWgpuContext>,
    core: PortCore,
    state: ConnectionState<ConnectedWriter>,
}

pub struct D2HThreadSafeConnectToken<D: CpuSample> {
    reader: PortEndpoint,
    _item: PhantomData<D>,
}

pub struct D2HThreadSafeReturnToken<D: CpuSample> {
    inbound: Arc<Mutex<Vec<OutputBufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<OutputBufferFull<D>>>>,
    context: Option<CubeWgpuContext>,
    connected: ConnectedReader,
}

impl<D: CpuSample> D2HWriter<D> {
    pub fn new() -> Self {
        Self {
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            context: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
        }
    }

    pub fn set_context(&mut self, context: CubeWgpuContext) {
        self.context = Some(context);
    }

    pub fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let Some(context) = self.context.as_ref() else {
            panic!("CubeCL D2H writer: set_context() must be called before injecting buffers");
        };
        let n_bytes = n_items * D::SIZE.get();
        let mut inbound = self.inbound.lock().unwrap();
        for _ in 0..n_buffers {
            inbound.push(OutputBufferEmpty {
                buffer: context.setup.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cubecl_d2h_readback_buffer"),
                    size: n_bytes as u64,
                    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                capacity: n_items,
                _p: PhantomData,
            });
        }
    }

    pub fn buffers(&mut self) -> Vec<OutputBufferEmpty<D>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    pub fn submit(&mut self, buffer: OutputBufferFull<D>) {
        self.outbound.lock().unwrap().push_back(buffer);
        self.state.connected().reader.inbox().notify();
    }
}

impl<D: CpuSample> Default for D2HWriter<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: CpuSample> BufferWriter for D2HWriter<D> {
    type Inbox = BlockInbox;
    type Reader = D2HReader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.context.is_none() {
            Err(Error::ValidationError(
                "CubeCL D2H writer: no context configured".to_string(),
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
        dest.context = self.context.clone();
        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
        });
        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
        });
    }

    async fn notify_finished(&mut self) {
        let reader = self.state.connected().reader.clone();
        let _ = reader.inbox().stream_input_done(reader.port_id()).await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D: CpuSample> ThreadSafeConnect for D2HWriter<D> {
    type ReaderToken = D2HThreadSafeConnectToken<D>;
    type WriterToken = D2HThreadSafeReturnToken<D>;

    fn take_reader_token(reader: &mut D2HReader<D>) -> Self::ReaderToken {
        D2HThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox(), reader.core.port_id()),
            _item: PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        self.state.set_connected(ConnectedWriter {
            reader: token.reader,
        });
        D2HThreadSafeReturnToken {
            inbound: self.inbound.clone(),
            outbound: self.outbound.clone(),
            context: self.context.clone(),
            connected: ConnectedReader {
                writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
            },
        }
    }

    fn finish_reader(reader: &mut D2HReader<D>, token: Self::WriterToken) {
        reader.inbound = token.outbound;
        reader.outbound = token.inbound;
        reader.context = token.context;
        reader.state.set_connected(token.connected);
    }
}

struct CurrentOutputBuffer<D: CpuSample> {
    buffer: OutputBufferFull<D>,
    byte_offset: usize,
    slice: BufferView,
}

pub struct D2HReader<D: CpuSample> {
    buffer: Option<CurrentOutputBuffer<D>>,
    inbound: Arc<Mutex<VecDeque<OutputBufferFull<D>>>>,
    outbound: Arc<Mutex<Vec<OutputBufferEmpty<D>>>>,
    context: Option<CubeWgpuContext>,
    core: PortCore,
    state: ConnectionState<ConnectedReader>,
    finished: bool,
}

impl<D: CpuSample> D2HReader<D> {
    pub fn new() -> Self {
        Self {
            buffer: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            context: None,
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
        }
    }

    pub fn set_context(&mut self, context: CubeWgpuContext) {
        self.context = Some(context);
    }
}

impl<D: CpuSample> Default for D2HReader<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: CpuSample> BufferReader for D2HReader<D> {
    type Inbox = BlockInbox;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.context.is_none() {
            Err(Error::ValidationError(
                "CubeCL D2H reader: no context configured".to_string(),
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
        let writer = self.state.connected().writer.clone();
        let _ = writer.inbox().stream_output_done(writer.port_id()).await;
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

impl<D: CpuSample + Pod> CpuBufferReader for D2HReader<D> {
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        static TAGS: Vec<ItemTag> = vec![];
        if self.buffer.is_none() {
            let Some(buffer) = self.inbound.lock().unwrap().pop_front() else {
                return (&[], &TAGS);
            };
            let slice = buffer
                .buffer
                .slice(0..buffer.used_bytes as u64)
                .get_mapped_range();
            self.buffer = Some(CurrentOutputBuffer {
                buffer,
                byte_offset: 0,
                slice,
            });
        }

        let buffer = self.buffer.as_ref().unwrap();
        let data = bytemuck::try_cast_slice(&buffer.slice[buffer.byte_offset..])
            .expect("CubeCL D2H reader: mapped buffer alignment invalid");
        (data, &TAGS)
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
            let CurrentOutputBuffer { buffer, slice, .. } = self.buffer.take().unwrap();
            drop(slice);
            let full = buffer;
            full.buffer.unmap();
            self.outbound.lock().unwrap().push(OutputBufferEmpty {
                buffer: full.buffer,
                capacity: full.used_bytes / D::SIZE.get(),
                _p: PhantomData,
            });
            self.state.connected().writer.inbox().notify();
            self.core.inbox().notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items is not implemented for CubeCL D2H buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items is not implemented for CubeCL D2H buffers");
    }

    fn max_items(&self) -> usize {
        usize::MAX
    }
}

impl<D: CpuSample> OutputBufferEmpty<D> {
    pub fn submit_full(self, used_bytes: usize) -> OutputBufferFull<D> {
        debug!(
            "CubeCL D2H output buffer full: used_bytes={}, capacity_items={}",
            used_bytes, self.capacity
        );
        OutputBufferFull {
            buffer: self.buffer,
            used_bytes,
            _p: PhantomData,
        }
    }
}
