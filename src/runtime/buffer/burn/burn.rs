use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitReturn;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::InplaceBuffer;
use crate::runtime::buffer::InplaceReader;
use crate::runtime::buffer::InplaceWriter;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::ThreadSafeConnect;
use crate::runtime::config::config;
use crate::runtime::dev::ItemTag;
use burn::prelude::*;
use burn::tensor::BasicOps;
use burn::tensor::TensorKind;
use bytemuck::Pod;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

type BufferPermits = Arc<AtomicUsize>;
type PermitReturn = CircuitReturn<BlockInbox, BufferPermits>;
type FullBuffers<B, E, SR> = Arc<Mutex<VecDeque<Buffer<B, E, SR>>>>;

enum BufferState<B, E = Float>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
{
    Tensor(Tensor<B, 1, E>),
    Data(TensorData),
}

/// In-place buffer
pub struct Buffer<B, E = Float, S = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    valid: usize,
    state: Option<BufferState<B, E>>,
    device: B::Device,
    tags: Vec<ItemTag>,
    permit_return: Option<PermitReturn>,
    _p: PhantomData<S>,
}

impl<B, E, S> Buffer<B, E, S>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    /// Create buffer
    ///
    /// The number of items corresponds to the number of items in the tensor.
    fn with_items(items: usize, device: &B::Device) -> Self {
        let data = TensorData::zeros::<E::Elem, _>([items]);
        Self {
            valid: 0,
            state: Some(BufferState::Data(data)),
            device: device.clone(),
            tags: Vec::new(),
            permit_return: None,
            _p: PhantomData,
        }
    }

    /// Create a Buffer from a Tensor
    pub fn from_tensor(tensor: Tensor<B, 1, E>) -> Self {
        let device = tensor.device();
        Self {
            valid: tensor.shape().num_elements(),
            state: Some(BufferState::Tensor(tensor)),
            device,
            tags: Vec::new(),
            permit_return: None,
            _p: PhantomData,
        }
    }

    /// Consume the buffer to create a Tensor
    pub fn into_tensor(mut self) -> Tensor<B, 1, E> {
        match self.state.take().expect("burn buffer state missing") {
            BufferState::Tensor(t) => t.slice(0..self.valid),
            BufferState::Data(d) => Tensor::from_data(d, &self.device).slice(0..self.valid),
        }
    }

    fn cast<SO: CpuSample>(mut self) -> Buffer<B, E, SO> {
        Buffer {
            valid: self.valid,
            state: self.state.take(),
            device: self.device.clone(),
            tags: std::mem::take(&mut self.tags),
            permit_return: self.permit_return.take(),
            _p: PhantomData,
        }
    }

    fn arm(&mut self, permit_return: PermitReturn) {
        self.permit_return = Some(permit_return);
    }

    fn has_permit(&self) -> bool {
        self.permit_return.is_some()
    }

    fn ensure_data(&mut self) {
        if matches!(self.state.as_ref(), Some(BufferState::Tensor(_)))
            && let BufferState::Tensor(t) = self.state.take().expect("burn buffer state missing")
        {
            self.state = Some(BufferState::Data(t.into_data()));
        }
    }

    /// Number of elements in the buffer
    pub fn num_tensor_elements(&self) -> usize {
        match self.state.as_ref().expect("burn buffer state missing") {
            BufferState::Tensor(t) => t.shape().num_elements(),
            BufferState::Data(d) => d.num_elements(),
        }
    }
    /// Number of elements in the buffer
    pub fn num_host_elements(&self) -> usize {
        let elem = self.num_tensor_elements();
        elem * size_of::<E::Elem>() / S::SIZE.get()
    }
}

impl<B, E, S> Drop for Buffer<B, E, S>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    fn drop(&mut self) {
        if let Some(permit_return) = self.permit_return.take() {
            permit_return.queue().fetch_add(1, Ordering::Release);
            permit_return.notify();
        }
    }
}

impl<B, E, S> InplaceBuffer for Buffer<B, E, S>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    E::Elem: Pod,
    S: CpuSample + Pod,
{
    type Item = S;

    fn set_valid(&mut self, valid: usize) {
        self.valid = valid * S::SIZE.get() / size_of::<E::Elem>();
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        self.ensure_data();
        match self.state.as_mut().expect("burn buffer state missing") {
            BufferState::Data(d) => {
                let s = &mut d.as_mut_slice::<E::Elem>().unwrap()[0..self.valid];
                bytemuck::try_cast_slice_mut(s)
                    .expect("burn buffer alignment invalid for host sample type")
            }
            _ => unreachable!(),
        }
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        self.ensure_data();
        match self.state.as_mut().expect("burn buffer state missing") {
            BufferState::Data(d) => {
                let s = &mut d.as_mut_slice::<E::Elem>().unwrap()[0..self.valid];
                let s = bytemuck::try_cast_slice_mut(s)
                    .expect("burn buffer alignment invalid for host sample type");
                (s, &mut self.tags)
            }
            _ => unreachable!(),
        }
    }
}

/// Burn Writer
pub struct Writer<B, E = Float, SW = f32, SR = SW>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    core: PortCore,
    state: ConnectionState<ConnectedWriter<B, E, SR>>,
    device: Option<Device<B>>,
    permits: BufferPermits,
    buffer_size_in_items: usize,
    current: Option<(Buffer<B, E, SW>, usize)>,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    reader: PortEndpoint,
    outbound: FullBuffers<B, E, SR>,
}

/// Reader offer for a Burn-buffer cross-domain connection.
#[doc(hidden)]
pub struct ThreadSafeConnectToken<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    reader: PortEndpoint,
    #[allow(clippy::type_complexity)]
    _marker: PhantomData<fn() -> (B, E, SR)>,
}

/// Reader installation returned by the Burn-buffer writer.
#[doc(hidden)]
pub struct ThreadSafeReturnToken<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    connected: ConnectedReader<B, E, SR>,
}

impl<B, E, SW, SR> Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    /// Create Burn buffer writer
    pub fn new() -> Self {
        Self {
            core: PortCore::with_requirements(BufferRequirements::with_min_items(1)),
            state: ConnectionState::disconnected(),
            device: None,
            permits: Arc::new(AtomicUsize::new(0)),
            buffer_size_in_items: config().buffer_size / SW::SIZE.get(),
            current: None,
            tags: Vec::new(),
        }
    }

    /// Set backend device
    ///
    /// This is required to create tensors
    pub fn set_device(&mut self, device: &B::Device) {
        self.device = Some(device.clone());
    }

    fn try_acquire_permit(&self) -> bool {
        self.permits
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
            .is_ok()
    }

    fn release_permit(&self) {
        self.permits.fetch_add(1, Ordering::Release);
        if self.core.is_bound() {
            self.core.inbox().notify();
        }
    }

    fn permit_return(&self) -> PermitReturn {
        CircuitReturn::new(self.core.inbox().clone(), self.permits.clone())
    }

    fn new_armed_buffer<S>(&self) -> Option<Buffer<B, E, S>>
    where
        E::Elem: Pod,
        S: CpuSample + Pod,
    {
        let Some(ref d) = self.device else {
            self.release_permit();
            warn!("cannot create buffers/tensors, device not set");
            return None;
        };
        let mut b = Buffer::with_items(self.buffer_size_in_items, d);
        b.set_valid(b.num_host_elements());
        b.arm(self.permit_return());
        Some(b)
    }
}

impl<B, E, SW, SR> Default for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B, E, SW, SR> BufferWriter for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    type Inbox = BlockInbox;
    type Reader = Reader<B, E, SR>;

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
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        let inbound = Arc::new(Mutex::new(VecDeque::new()));

        self.state.set_connected(ConnectedWriter {
            reader: PortEndpoint::new(dest.core.inbox().clone(), dest.core.port_id()),
            outbound: inbound.clone(),
        });

        dest.state.set_connected(ConnectedReader {
            writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
            inbound,
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

impl<B, E, SW, SR> ThreadSafeConnect for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<B, E, SR>;
    type WriterToken = ThreadSafeReturnToken<B, E, SR>;

    fn take_reader_token(reader: &mut Reader<B, E, SR>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox().clone(), reader.core.port_id()),
            _marker: PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        let inbound = Arc::new(Mutex::new(VecDeque::new()));
        self.state.set_connected(ConnectedWriter {
            reader: token.reader,
            outbound: inbound.clone(),
        });
        ThreadSafeReturnToken {
            connected: ConnectedReader {
                writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
                inbound,
            },
        }
    }

    fn finish_reader(reader: &mut Reader<B, E, SR>, token: Self::WriterToken) {
        reader.state.set_connected(token.connected);
    }
}

impl<B, E, SW, SR> InplaceWriter for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    E::Elem: Pod,
    SW: CpuSample + Pod,
    SR: CpuSample + Pod,
{
    type Item = SW;
    type Buffer = Buffer<B, E, SW>;

    fn put_full_buffer(&mut self, mut buffer: Self::Buffer) -> Result<(), Error> {
        if !buffer.has_permit() {
            if !self.try_acquire_permit() {
                return Err(Error::RuntimeError(
                    "cannot submit burn buffer, no empty-buffer permit available".to_string(),
                ));
            }
            buffer.arm(self.permit_return());
        }
        let connected = self.state.connected();
        connected.outbound.lock().unwrap().push_back(buffer.cast());
        connected.reader.inbox().notify();
        Ok(())
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        if self.try_acquire_permit() {
            self.new_armed_buffer()
        } else {
            None
        }
    }

    fn has_more_buffers(&mut self) -> bool {
        self.permits.load(Ordering::Acquire) > 0
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        self.buffer_size_in_items = n_items;
        self.permits.fetch_add(n_buffers, Ordering::Release);
        if n_buffers > 0 && self.core.is_bound() {
            self.core.inbox().notify();
        }
    }
}

impl<B, E, SW, SR> CpuBufferWriter for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    E::Elem: Pod,
    SW: CpuSample + Pod,
    SR: CpuSample + Pod,
{
    type Item = SW;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            if self.try_acquire_permit() {
                self.current = self.new_armed_buffer().map(|b| (b, 0));
                if self.current.is_none() {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
            } else {
                return (&mut [], Tags::new(&mut self.tags, 0));
            }
        }

        let (b, o) = self.current.as_mut().unwrap();
        let (s, t) = b.slice_with_tags();
        (&mut s[*o..], Tags::new(t, 0))
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let (c, o) = self.current.as_mut().unwrap();
        debug_assert!(n <= c.num_host_elements() - *o);
        *o += n;

        if (c.num_host_elements() - *o) < self.core.min_items().unwrap_or(1) {
            let (mut c, o) = self.current.take().unwrap();
            c.set_valid(o);
            let connected = self.state.connected();
            connected.outbound.lock().unwrap().push_back(c.cast());

            connected.reader.inbox().notify();

            if self.permits.load(Ordering::Acquire) > 0 {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        self.core.raise_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.core
            .raise_min_buffer_size_in_items(std::cmp::max(n, 1));
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for burn writer");
        1
    }
}

/// Burn Reader
pub struct Reader<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    core: PortCore,
    state: ConnectionState<ConnectedReader<B, E, SR>>,
    finished: bool,
    current: Option<(Buffer<B, E, SR>, usize)>,
}

struct ConnectedReader<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    writer: PortEndpoint,
    inbound: FullBuffers<B, E, SR>,
}

impl<B, E, SR> Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    /// Create Burn buffer reader
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
            current: None,
        }
    }
}

impl<B, E, SR> Default for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B, E, SR> BufferReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
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
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    async fn notify_finished(&mut self) {
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
        self.finished
            && self
                .state
                .as_ref()
                .is_none_or(|state| state.inbound.lock().unwrap().is_empty())
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<B, E, SR> InplaceReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    E::Elem: Pod,
    SR: CpuSample + Pod,
{
    type Item = SR;
    type Buffer = Buffer<B, E, SR>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        self.state.connected().inbound.lock().unwrap().pop_front()
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.state.connected().inbound.lock().unwrap().is_empty()
    }
}

impl<B, E, SR> CpuBufferReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    E::Elem: Pod,
    SR: CpuSample + Pod,
{
    type Item = SR;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            match self.state.connected().inbound.lock().unwrap().pop_front() {
                Some(b) => {
                    self.current = Some((b, 0));
                }
                None => {
                    static V: Vec<ItemTag> = vec![];
                    return (&[], &V);
                }
            }
        }

        let (c, o) = self.current.as_mut().unwrap();
        let (s, t) = c.slice_with_tags();
        (&s[*o..], t)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let (c, o) = self.current.as_mut().unwrap();
        debug_assert!(n <= c.valid - *o);
        *o += n;

        if *o == c.valid {
            let _ = self.current.take().unwrap();

            if !self.state.connected().inbound.lock().unwrap().is_empty() {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not implemented for burn reader");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not implemented for burn reader");
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for burn reader");
        1
    }
}
