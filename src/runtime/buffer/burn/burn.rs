use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::InplaceBuffer;
use crate::runtime::buffer::InplaceReader;
use crate::runtime::buffer::InplaceWriter;
use crate::runtime::buffer::Tags;
use crate::runtime::config::config;
use burn::prelude::*;
use burn::tensor::BasicOps;
use burn::tensor::TensorKind;
use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

enum BufferState<B, E = Float, S = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    Tensor(Tensor<B, 1, E>),
    Data(TensorData),
    Empty(PhantomData<S>),
}

impl<B, E, S> BufferState<B, E, S>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    fn cast<SO: CpuSample>(self) -> BufferState<B, E, SO> {
        match self {
            BufferState::Tensor(t) => BufferState::Tensor(t),
            BufferState::Data(d) => BufferState::Data(d),
            BufferState::Empty(_) => BufferState::Empty(PhantomData),
        }
    }
}

/// In-place buffer
pub struct Buffer<B, E = Float, S = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    valid: usize,
    state: BufferState<B, E, S>,
    device: B::Device,
    tags: Vec<ItemTag>,
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
            state: BufferState::Data(data),
            device: device.clone(),
            tags: Vec::new(),
        }
    }

    /// Create a Buffer from a Tensor
    pub fn from_tensor(tensor: Tensor<B, 1, E>) -> Self {
        let device = tensor.device();
        Self {
            valid: tensor.shape().num_elements(),
            state: BufferState::Tensor(tensor),
            device,
            tags: Vec::new(),
        }
    }

    /// Consume the buffer to create a Tensor
    pub fn into_tensor(self) -> Tensor<B, 1, E> {
        match self.state {
            BufferState::Tensor(t) => t.slice(0..self.valid),
            BufferState::Data(d) => Tensor::from_data(d, &self.device).slice(0..self.valid),
            BufferState::Empty(_) => unreachable!(),
        }
    }

    fn cast<SO: CpuSample>(self) -> Buffer<B, E, SO> {
        let Self {
            valid,
            state,
            device,
            tags,
        } = self;
        Buffer {
            valid,
            state: state.cast(),
            device,
            tags,
        }
    }

    fn ensure_data(&mut self) {
        if matches!(self.state, BufferState::Tensor(_))
            && let BufferState::Tensor(t) =
                std::mem::replace(&mut self.state, BufferState::Empty(PhantomData))
        {
            self.state = BufferState::Data(t.into_data());
        }
    }

    /// Number of elements in the buffer
    pub fn num_tensor_elements(&self) -> usize {
        match &self.state {
            BufferState::Tensor(t) => t.shape().num_elements(),
            BufferState::Data(d) => d.num_elements(),
            BufferState::Empty(_) => unreachable!(),
        }
    }
    /// Number of elements in the buffer
    pub fn num_host_elements(&self) -> usize {
        let elem = self.num_tensor_elements();
        elem * size_of::<E::Elem>() / size_of::<S>()
    }
}

impl<B, E, S> InplaceBuffer for Buffer<B, E, S>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    S: CpuSample,
{
    type Item = S;

    fn set_valid(&mut self, valid: usize) {
        self.valid = valid * size_of::<S>() / size_of::<E::Elem>();
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        self.ensure_data();
        match self.state {
            BufferState::Data(ref mut d) => {
                let s = &mut d.as_mut_slice::<E::Elem>().unwrap()[0..self.valid];
                let len = size_of_val(s) / size_of::<S>();
                unsafe { std::slice::from_raw_parts_mut(s.as_mut_ptr() as *mut S, len) }
            }
            _ => unreachable!(),
        }
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        self.ensure_data();
        match self.state {
            BufferState::Data(ref mut d) => {
                let s = &mut d.as_mut_slice::<E::Elem>().unwrap()[0..self.valid];
                let len = size_of_val(s) / size_of::<S>();
                let s = unsafe { std::slice::from_raw_parts_mut(s.as_mut_ptr() as *mut S, len) };
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
    reader_inbox: Sender<BlockMessage>,
    reader_input: PortId,
    writer_inbox: Sender<BlockMessage>,
    writer_id: BlockId,
    writer_output: PortId,
    device: Option<Device<B>>,
    #[allow(clippy::type_complexity)]
    inbound: Arc<Mutex<Vec<Option<Buffer<B, E, SR>>>>>,
    outbound: Arc<Mutex<VecDeque<Buffer<B, E, SR>>>>,
    buffer_size_in_items: usize,
    // for CPU buffer writer
    current: Option<(Buffer<B, E, SW>, usize)>,
    min_items: usize,
    min_buffer_size_in_items: Option<usize>,
    // dummy to return when no buffer available
    tags: Vec<ItemTag>,
}

impl<B, E, SW, SR> Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    /// Create circuit buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            reader_inbox: rx.clone(),
            reader_input: PortId::default(),
            writer_inbox: rx,
            writer_id: BlockId::default(),
            writer_output: PortId::default(),
            device: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            buffer_size_in_items: config().buffer_size / std::mem::size_of::<SW>(),
            current: None,
            min_items: 1,
            min_buffer_size_in_items: None,
            tags: Vec::new(),
        }
    }

    /// Set backend device
    ///
    /// This is required to create tensors
    pub fn set_device(&mut self, device: &B::Device) {
        self.device = Some(device.clone());
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut Reader<B, E, SR>) {
        end.circuit_start = Some((self.writer_inbox.clone(), self.inbound.clone()));
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
    type Reader = Reader<B, E, SR>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.writer_id = block_id;
        self.writer_output = port_id;
        self.writer_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.reader_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.writer_id, self.writer_output
            )))
        } else {
            Ok(())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        self.reader_input = dest.reader_input.clone();
        self.reader_inbox = dest.reader_inbox.clone();
        self.outbound = dest.inbound.clone();

        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_output = self.writer_output.clone();
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output.clone()
    }
}

impl<B, E, SW, SR> InplaceWriter for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    type Item = SW;
    type Buffer = Buffer<B, E, SW>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) {
        self.outbound.lock().unwrap().push_back(buffer.cast());
        let _ = self.reader_inbox.try_send(BlockMessage::Notify);
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        self.inbound.lock().unwrap().pop().and_then(|b| {
            let b: Option<Buffer<B, E, SW>> = b.map(Buffer::cast);
            if let Some(mut b) = b {
                b.set_valid(b.num_host_elements());
                Some(b)
            } else if let Some(ref d) = self.device {
                let mut b = Buffer::with_items(self.buffer_size_in_items, d);
                b.set_valid(b.num_host_elements());
                Some(b)
            } else {
                warn!("cannot create buffers/tensors, device not set");
                None
            }
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.lock().unwrap().is_empty()
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        self.buffer_size_in_items = n_items;
        if let Some(ref d) = self.device {
            let mut q = self.inbound.lock().unwrap();
            for _ in 0..n_buffers {
                q.push(Some(Buffer::with_items(n_items, d)));
            }
        } else {
            warn!("cannot create buffers/tensors, device not set");
        }
    }
}

impl<B, E, SW, SR> CpuBufferWriter for Writer<B, E, SW, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SW: CpuSample,
    SR: CpuSample,
{
    type Item = SW;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            match self.inbound.lock().unwrap().pop() {
                Some(Some(mut b)) => {
                    b.valid = b.num_tensor_elements();
                    b.tags.clear();
                    self.current = Some((b.cast(), 0));
                }
                Some(None) => {
                    if let Some(ref d) = self.device {
                        let mut b = Buffer::with_items(self.buffer_size_in_items, d);
                        b.set_valid(b.num_host_elements());
                        self.current = Some((b, 0));
                    } else {
                        warn!("cannot create buffer, device not set");
                    }
                }
                None => {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
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

        if (c.num_host_elements() - *o) < self.min_items {
            let (mut c, o) = self.current.take().unwrap();
            c.set_valid(o);
            self.outbound.lock().unwrap().push_back(c.cast());

            let _ = self.reader_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again, if we have another buffer queued
            if !self.inbound.lock().unwrap().is_empty() {
                let _ = self.writer_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        self.min_items = std::cmp::max(self.min_items, n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.min_buffer_size_in_items = match self.min_buffer_size_in_items {
            Some(c) => Some(std::cmp::max(n, c)),
            None => Some(std::cmp::max(n, 1)),
        }
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for circuit writer");
        1
    }
}

/// Circuit Reader
pub struct Reader<B, E = Float, SR = f32>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    reader_inbox: Sender<BlockMessage>,
    reader_id: BlockId,
    reader_input: PortId,
    writer_inbox: Sender<BlockMessage>,
    writer_output: PortId,
    inbound: Arc<Mutex<VecDeque<Buffer<B, E, SR>>>>,
    #[allow(clippy::type_complexity)]
    circuit_start: Option<(
        Sender<BlockMessage>,
        Arc<Mutex<Vec<Option<Buffer<B, E, SR>>>>>,
    )>,
    finished: bool,
    // for CPU buffer reader
    current: Option<(Buffer<B, E, SR>, usize)>,
}

impl<B, E, SR> Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    /// Create circuit buffer reader
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            reader_inbox: rx.clone(),
            reader_id: BlockId::default(),
            reader_input: PortId::default(),
            writer_inbox: rx,
            writer_output: PortId::default(),
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            circuit_start: None,
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

#[async_trait]
impl<B, E, SR> BufferReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.reader_id = block_id;
        self.reader_input = port_id;
        self.reader_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.reader_id, self.reader_input
            )))
        } else {
            Ok(())
        }
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output.clone(),
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
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input.clone()
    }
}

impl<B, E, SR> InplaceReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    type Item = SR;

    type Buffer = Buffer<B, E, SR>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        self.inbound.lock().unwrap().pop_front()
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.lock().unwrap().is_empty()
    }

    fn put_empty_buffer(&mut self, buffer: Self::Buffer) {
        if let Some((ref mut inbox, ref buffers)) = self.circuit_start {
            buffers.lock().unwrap().push(Some(buffer));
            let _ = inbox.try_send(BlockMessage::Notify);
        }
    }

    fn notify_consumed_buffer(&mut self) {
        if let Some((ref mut inbox, ref buffers)) = self.circuit_start {
            buffers.lock().unwrap().push(None);
            let _ = inbox.try_send(BlockMessage::Notify);
        }
    }
}

impl<B, E, SR> CpuBufferReader for Reader<B, E, SR>
where
    B: Backend,
    E: TensorKind<B> + BasicOps<B> + Send + Sync + 'static,
    SR: CpuSample,
{
    type Item = SR;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            match self.inbound.lock().unwrap().pop_front() {
                Some(b) => {
                    self.current = Some((b, 0));
                }
                _ => {
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
            let (b, _) = self.current.take().unwrap();
            match self.circuit_start {
                Some((ref mut inbox, ref queue)) => {
                    queue.lock().unwrap().push(Some(b));
                    let _ = inbox.try_send(BlockMessage::Notify);
                }
                None => {
                    debug!(
                        "burn reader used as cpu buffer reader but not connected to circuit start. dropping buffer."
                    );
                }
            }

            // make sure to be called again, if we have another buffer queued
            if !self.inbound.lock().unwrap().is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not implemented for circuit reader");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not implemented for circuit reader");
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for circuit reader");
        1
    }
}
