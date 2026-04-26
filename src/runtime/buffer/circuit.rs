use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitReturn;
use crate::runtime::buffer::CircuitWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::InplaceBuffer;
use crate::runtime::buffer::InplaceReader;
use crate::runtime::buffer::InplaceWriter;
use crate::runtime::buffer::PortConfig;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::config::config;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::ItemTag;
#[cfg(not(target_arch = "wasm32"))]
use concurrent_queue::ConcurrentQueue;
use std::any::Any;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
type Queue<T> = ConcurrentQueue<T>;
#[cfg(target_arch = "wasm32")]
type Queue<T> = Mutex<VecDeque<T>>;
type EmptyBuffers<T> = Arc<Queue<Option<Buffer<T>>>>;
type FullBuffers<T> = Arc<Queue<Buffer<T>>>;

fn queue_new<T>() -> Queue<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ConcurrentQueue::bounded(1024)
    }
    #[cfg(target_arch = "wasm32")]
    {
        Mutex::new(VecDeque::new())
    }
}

fn queue_push<T>(queue: &Queue<T>, item: T) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if queue.push(item).is_err() {
            panic!("circuit queue push failed (full or closed)");
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        queue.lock().unwrap().push_back(item);
    }
}

fn queue_pop<T>(queue: &Queue<T>) -> Option<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        queue.pop().ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        queue.lock().unwrap().pop_front()
    }
}

fn queue_pop_back<T>(queue: &Queue<T>) -> Option<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        queue.pop().ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        queue.lock().unwrap().pop_back()
    }
}

fn queue_is_empty<T>(queue: &Queue<T>) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        queue.is_empty()
    }
    #[cfg(target_arch = "wasm32")]
    {
        queue.lock().unwrap().is_empty()
    }
}

/// In-place buffer
pub struct Buffer<T>
where
    T: CpuSample,
{
    valid: usize,
    buffer: Box<[T]>,
    tags: Vec<ItemTag>,
}

impl<T> Buffer<T>
where
    T: CpuSample,
{
    /// Create buffer
    fn with_items(items: usize) -> Self {
        Self {
            valid: 0,
            buffer: vec![T::default(); items].into_boxed_slice(),
            tags: Vec::new(),
        }
    }
}

impl<T> InplaceBuffer for Buffer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn set_valid(&mut self, valid: usize) {
        self.valid = valid;
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        &mut self.buffer[0..self.valid]
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        (&mut self.buffer[0..self.valid], &mut self.tags)
    }
}

/// Circuit Writer
pub struct Writer<T>
where
    T: CpuSample,
{
    core: PortCore,
    state: ConnectionState<ConnectedWriter<T>>,
    inbound: EmptyBuffers<T>,
    buffer_size_in_items: usize,
    current: Option<Buffer<T>>,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<T>
where
    T: CpuSample,
{
    reader: PortEndpoint,
    outbound: FullBuffers<T>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    /// Create circuit buffer writer
    pub fn new() -> Self {
        Self {
            core: PortCore::with_config(PortConfig::with_min_items(1)),
            state: ConnectionState::disconnected(),
            inbound: Arc::new(queue_new()),
            buffer_size_in_items: config().buffer_size / std::mem::size_of::<T>(),
            current: None,
            tags: Vec::new(),
        }
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut Reader<T>) {
        end.circuit_start = Some(CircuitReturn::new(
            self.core.notifier(),
            self.inbound.clone(),
        ));
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

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
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
        let inbound = Arc::new(queue_new());

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
        if let Some(b) = self.current.take() {
            queue_push(&self.state.connected().outbound, b);
            self.state.connected().reader.inbox().notify();
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
    T: CpuSample,
{
    type CircuitEnd = Reader<T>;

    fn close_circuit(&mut self, dst: &mut Self::CircuitEnd) {
        dst.circuit_start = Some(CircuitReturn::new(
            self.core.notifier(),
            self.inbound.clone(),
        ));
    }
}

impl<T> InplaceWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;
    type Buffer = Buffer<T>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) {
        queue_push(&self.state.connected().outbound, buffer);
        self.state.connected().reader.inbox().notify();
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        queue_pop_back(&self.inbound).map(|b| {
            if let Some(mut b) = b {
                b.valid = b.buffer.len();
                b.tags.clear();
                b
            } else {
                Buffer::with_items(self.buffer_size_in_items)
            }
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !queue_is_empty(&self.inbound)
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        self.buffer_size_in_items = n_items;
        for _ in 0..n_buffers {
            queue_push(&self.inbound, Some(Buffer::with_items(n_items)));
        }
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            match queue_pop_back(&self.inbound) {
                Some(Some(mut b)) => {
                    b.valid = 0;
                    b.tags.clear();
                    self.current = Some(b);
                }
                Some(None) => {
                    self.current = Some(Buffer::with_items(self.buffer_size_in_items));
                }
                None => {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
            }
        }

        let c = self.current.as_mut().unwrap();
        (&mut c.buffer[c.valid..], Tags::new(&mut c.tags, c.valid))
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.buffer.len() - c.valid);
        c.valid += n;
        if (c.buffer.len() - c.valid) < self.core.min_items().unwrap_or(1) {
            let c = self.current.take().unwrap();
            queue_push(&self.state.connected().outbound, c);

            self.state.connected().reader.inbox().notify();

            if !queue_is_empty(&self.inbound) {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        self.core.set_min_items_max(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.core
            .set_min_buffer_size_in_items_max(std::cmp::max(n, 1));
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for circuit writer");
        1
    }
}

/// Circuit Reader
pub struct Reader<T>
where
    T: CpuSample,
{
    core: PortCore,
    state: ConnectionState<ConnectedReader<T>>,
    circuit_start: Option<CircuitReturn<EmptyBuffers<T>>>,
    finished: bool,
    current: Option<(Buffer<T>, usize)>,
}

struct ConnectedReader<T>
where
    T: CpuSample,
{
    writer: PortEndpoint,
    inbound: FullBuffers<T>,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    /// Create circuit buffer reader
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            circuit_start: None,
            finished: false,
            current: None,
        }
    }
}

impl<T> Default for Reader<T>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
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
            && self
                .state
                .as_ref()
                .is_none_or(|state| queue_is_empty(&state.inbound))
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<T> InplaceReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;
    type Buffer = Buffer<T>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        queue_pop(&self.state.connected().inbound)
    }

    fn has_more_buffers(&mut self) -> bool {
        !queue_is_empty(&self.state.connected().inbound)
    }

    fn put_empty_buffer(&mut self, mut buffer: Self::Buffer) {
        buffer.tags.clear();
        if let Some(circuit_start) = self.circuit_start.as_ref() {
            queue_push(circuit_start.queue(), Some(buffer));
            circuit_start.notify();
        } else {
            warn!("Put empty buffer in unconnected circuit reader. Dropping buffer.")
        }
    }

    fn notify_consumed_buffer(&mut self) {
        if let Some(circuit_start) = self.circuit_start.as_ref() {
            queue_push(circuit_start.queue(), None);
            circuit_start.notify();
        } else {
            warn!("Dropped buffer in unconnected circuit reader. Dropping buffer.")
        }
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            match queue_pop(&self.state.connected().inbound) {
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
        (&c.buffer[*o..c.valid], &c.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let (c, o) = self.current.as_mut().unwrap();
        debug_assert!(n <= c.valid - *o);
        *o += n;

        if *o == c.valid {
            let (mut b, _) = self.current.take().unwrap();
            b.tags.clear();
            match self.circuit_start.as_ref() {
                Some(circuit_start) => {
                    queue_push(circuit_start.queue(), Some(b));
                    circuit_start.notify();
                }
                None => {
                    warn!(
                        "circuit reader used as cpu buffer reader but not connected to circuit start. dropping buffer."
                    );
                }
            }

            if !queue_is_empty(&self.state.connected().inbound) {
                self.core.inbox().notify();
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
