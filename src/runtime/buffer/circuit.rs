#[cfg(not(target_arch = "wasm32"))]
use concurrent_queue::ConcurrentQueue;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferInbox;
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

#[cfg(not(target_arch = "wasm32"))]
type Queue<T> = ConcurrentQueue<T>;
#[cfg(target_arch = "wasm32")]
type Queue<T> = Mutex<VecDeque<T>>;
type EmptyBuffers<T, I> = Arc<Queue<Buffer<T, I>>>;
type FullBuffers<T, I> = Arc<Queue<Buffer<T, I>>>;

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

fn queue_try_push<T>(queue: &Queue<T>, item: T) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        queue.push(item).is_ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        queue.lock().unwrap().push_back(item);
        true
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

/// In-place buffer storage.
struct BufferStorage<T>
where
    T: CpuSample,
{
    valid: usize,
    buffer: Box<[T]>,
    tags: Vec<ItemTag>,
}

impl<T> BufferStorage<T>
where
    T: CpuSample,
{
    fn with_items(items: usize) -> Self {
        Self {
            valid: 0,
            buffer: vec![T::default(); items].into_boxed_slice(),
            tags: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.valid = 0;
        self.tags.clear();
    }
}

/// In-place buffer.
///
/// Buffers remember the writer queue they originated from while they are in
/// flight. If the final owner drops the buffer instead of forwarding it, the
/// buffer automatically returns to that origin queue.
pub struct Buffer<T, I = BlockInbox>
where
    T: CpuSample,
    I: BufferInbox,
{
    storage: Option<BufferStorage<T>>,
    origin: Option<CircuitReturn<I, EmptyBuffers<T, I>>>,
}

impl<T, I> Buffer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    /// Create buffer.
    fn with_items(items: usize) -> Self {
        Self {
            storage: Some(BufferStorage::with_items(items)),
            origin: None,
        }
    }

    fn storage(&self) -> &BufferStorage<T> {
        self.storage
            .as_ref()
            .expect("circuit buffer storage missing")
    }

    fn storage_mut(&mut self) -> &mut BufferStorage<T> {
        self.storage
            .as_mut()
            .expect("circuit buffer storage missing")
    }

    fn arm(&mut self, origin: CircuitReturn<I, EmptyBuffers<T, I>>) {
        self.origin = Some(origin);
    }
}

impl<T, I> Drop for Buffer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    fn drop(&mut self) {
        let Some(origin) = self.origin.take() else {
            return;
        };
        let Some(mut storage) = self.storage.take() else {
            return;
        };
        storage.reset();
        let returned = Buffer {
            storage: Some(storage),
            origin: None,
        };
        if queue_try_push(origin.queue(), returned) {
            origin.notify();
        }
    }
}

impl<T, I> InplaceBuffer for Buffer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Item = T;

    fn set_valid(&mut self, valid: usize) {
        self.storage_mut().valid = valid;
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        let storage = self.storage_mut();
        &mut storage.buffer[0..storage.valid]
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        let storage = self.storage_mut();
        (&mut storage.buffer[0..storage.valid], &mut storage.tags)
    }
}

/// Circuit Writer
pub struct Writer<T, I = BlockInbox>
where
    T: CpuSample,
    I: BufferInbox,
{
    core: PortCore<I>,
    state: ConnectionState<ConnectedWriter<T, I>>,
    inbound: EmptyBuffers<T, I>,
    buffer_size_in_items: usize,
    current: Option<Buffer<T, I>>,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    reader: PortEndpoint<I>,
    outbound: FullBuffers<T, I>,
}

/// Reader offer for an in-place cross-domain connection.
#[doc(hidden)]
pub struct ThreadSafeConnectToken<T>
where
    T: CpuSample,
{
    reader: PortEndpoint<BlockInbox>,
    _item: std::marker::PhantomData<T>,
}

/// Reader installation returned by the in-place writer.
#[doc(hidden)]
pub struct ThreadSafeReturnToken<T>
where
    T: CpuSample,
{
    connected: ConnectedReader<T, BlockInbox>,
}

impl<T, I> Writer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    /// Create circuit buffer writer
    pub fn new() -> Self {
        Self {
            core: PortCore::with_requirements(BufferRequirements::with_min_items(1)),
            state: ConnectionState::disconnected(),
            inbound: Arc::new(queue_new()),
            buffer_size_in_items: config().buffer_size / T::SIZE.get(),
            current: None,
            tags: Vec::new(),
        }
    }
}

impl<T, I> Default for Writer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, I> BufferWriter for Writer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Inbox = I;
    type Reader = Reader<T, I>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: I) {
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
        let inbound = Arc::new(queue_new());

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
        let connected = self.state.connected();
        if let Some(b) = self.current.take() {
            queue_push(&connected.outbound, b);
            connected.reader.inbox().notify();
        }
        let _ = connected
            .reader
            .inbox()
            .stream_input_done(connected.reader.port_id())
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<T> ThreadSafeConnect for Writer<T, BlockInbox>
where
    T: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<T>;
    type WriterToken = ThreadSafeReturnToken<T>;

    fn take_reader_token(reader: &mut Reader<T, BlockInbox>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox().clone(), reader.core.port_id()),
            _item: std::marker::PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        let inbound = Arc::new(queue_new());
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

    fn finish_reader(reader: &mut Reader<T, BlockInbox>, token: Self::WriterToken) {
        reader.state.set_connected(token.connected);
    }
}

impl<T, I> InplaceWriter for Writer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Item = T;
    type Buffer = Buffer<T, I>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) -> Result<(), Error> {
        let connected = self.state.connected();
        queue_push(&connected.outbound, buffer);
        connected.reader.inbox().notify();
        Ok(())
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        queue_pop_back(&self.inbound).map(|mut buffer| {
            let storage = buffer.storage_mut();
            storage.valid = storage.buffer.len();
            storage.tags.clear();
            buffer.arm(CircuitReturn::new(
                self.core.inbox().clone(),
                self.inbound.clone(),
            ));
            buffer
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !queue_is_empty(&self.inbound)
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        self.buffer_size_in_items = n_items;
        for _ in 0..n_buffers {
            queue_push(&self.inbound, Buffer::with_items(n_items));
        }
    }
}

impl<T, I> CpuBufferWriter for Writer<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            match queue_pop_back(&self.inbound) {
                Some(mut buffer) => {
                    buffer.storage_mut().reset();
                    buffer.arm(CircuitReturn::new(
                        self.core.inbox().clone(),
                        self.inbound.clone(),
                    ));
                    self.current = Some(buffer);
                }
                None => {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
            }
        }

        let c = self.current.as_mut().unwrap();
        let storage = c.storage_mut();
        let valid = storage.valid;
        (
            &mut storage.buffer[valid..],
            Tags::new(&mut storage.tags, valid),
        )
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let c = self.current.as_mut().unwrap();
        let storage = c.storage_mut();
        debug_assert!(n <= storage.buffer.len() - storage.valid);
        storage.valid += n;
        if (storage.buffer.len() - storage.valid) < self.core.min_items().unwrap_or(1) {
            let c = self.current.take().unwrap();
            let connected = self.state.connected();
            queue_push(&connected.outbound, c);
            connected.reader.inbox().notify();

            if !queue_is_empty(&self.inbound) {
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
        warn!("max_items not implemented for circuit writer");
        1
    }
}

/// Circuit Reader
pub struct Reader<T, I = BlockInbox>
where
    T: CpuSample,
    I: BufferInbox,
{
    core: PortCore<I>,
    state: ConnectionState<ConnectedReader<T, I>>,
    finished: bool,
    current: Option<(Buffer<T, I>, usize)>,
}

struct ConnectedReader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    writer: PortEndpoint<I>,
    inbound: FullBuffers<T, I>,
}

impl<T, I> Reader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    /// Create circuit buffer reader
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
            current: None,
        }
    }
}

impl<T, I> Default for Reader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, I> BufferReader for Reader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Inbox = I;
    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: I) {
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
        let _ = writer.inbox().stream_output_done(writer.port_id()).await;
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

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<T, I> InplaceReader for Reader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
{
    type Item = T;
    type Buffer = Buffer<T, I>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        queue_pop(&self.state.connected().inbound)
    }

    fn has_more_buffers(&mut self) -> bool {
        !queue_is_empty(&self.state.connected().inbound)
    }
}

impl<T, I> CpuBufferReader for Reader<T, I>
where
    T: CpuSample,
    I: BufferInbox,
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
        let storage = c.storage();
        (&storage.buffer[*o..storage.valid], &storage.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let (c, o) = self.current.as_mut().unwrap();
        let valid = c.storage().valid;
        debug_assert!(n <= valid - *o);
        *o += n;

        if *o == valid {
            let _ = self.current.take().unwrap();

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
