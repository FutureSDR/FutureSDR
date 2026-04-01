use crate::runtime::BlockId;
use crate::runtime::BlockInbox;
use crate::runtime::BlockMessage;
use crate::runtime::BlockNotifier;
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
        // No pop_back for concurrent queue. Use pop as FIFO.
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
    reader_inbox: BlockInbox,
    reader_input: PortId,
    writer_inbox: BlockInbox,
    notifier: BlockNotifier,
    reader_notifier: BlockNotifier,
    writer_id: BlockId,
    writer_output: PortId,
    inbound: Arc<Queue<Option<Buffer<T>>>>,
    outbound: Arc<Queue<Buffer<T>>>,
    buffer_size_in_items: usize,
    // for CPU buffer writer
    current: Option<Buffer<T>>,
    min_items: usize,
    min_buffer_size_in_items: Option<usize>,
    // dummy to return when no buffer available
    tags: Vec<ItemTag>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    /// Create circuit buffer writer
    pub fn new() -> Self {
        Self {
            reader_inbox: BlockInbox::default(),
            reader_input: PortId::default(),
            writer_inbox: BlockInbox::default(),
            notifier: BlockNotifier::new(),
            reader_notifier: BlockNotifier::new(),
            writer_id: BlockId::default(),
            writer_output: PortId::default(),
            inbound: Arc::new(queue_new()),
            outbound: Arc::new(queue_new()),
            buffer_size_in_items: config().buffer_size / std::mem::size_of::<T>(),
            current: None,
            min_items: 1,
            min_buffer_size_in_items: None,
            tags: Vec::new(),
        }
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut Reader<T>) {
        end.circuit_start = Some((self.notifier.clone(), self.inbound.clone()));
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.writer_id = block_id;
        self.writer_output = port_id;
        self.notifier = inbox.notifier();
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
        self.reader_notifier = dest.notifier.clone();

        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_notifier = self.notifier.clone();
        dest.writer_output = self.writer_output.clone();
    }

    async fn notify_finished(&mut self) {
        if let Some(b) = self.current.take() {
            queue_push(&self.outbound, b);
            self.reader_notifier.notify();
        }
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

impl<T> InplaceWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    type Buffer = Buffer<T>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) {
        queue_push(&self.outbound, buffer);
        self.reader_notifier.notify();
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
        if (c.buffer.len() - c.valid) < self.min_items {
            let c = self.current.take().unwrap();
            queue_push(&self.outbound, c);

            self.reader_notifier.notify();

            // make sure to be called again, if we have another buffer queued
            if !queue_is_empty(&self.inbound) {
                self.notifier.notify();
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
pub struct Reader<T>
where
    T: CpuSample,
{
    reader_inbox: BlockInbox,
    reader_id: BlockId,
    reader_input: PortId,
    writer_inbox: BlockInbox,
    notifier: BlockNotifier,
    writer_notifier: BlockNotifier,
    writer_output: PortId,
    inbound: Arc<Queue<Buffer<T>>>,
    #[allow(clippy::type_complexity)]
    circuit_start: Option<(BlockNotifier, Arc<Queue<Option<Buffer<T>>>>)>,
    finished: bool,
    // for CPU buffer reader
    current: Option<(Buffer<T>, usize)>,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    /// Create circuit buffer reader
    pub fn new() -> Self {
        Self {
            reader_inbox: BlockInbox::default(),
            reader_id: BlockId::default(),
            reader_input: PortId::default(),
            writer_inbox: BlockInbox::default(),
            notifier: BlockNotifier::new(),
            writer_notifier: BlockNotifier::new(),
            writer_output: PortId::default(),
            inbound: Arc::new(queue_new()),
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.reader_id = block_id;
        self.reader_input = port_id;
        self.notifier = inbox.notifier();
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
        self.finished && queue_is_empty(&self.inbound)
    }

    fn block_id(&self) -> BlockId {
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input.clone()
    }
}

impl<T> InplaceReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    type Buffer = Buffer<T>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        queue_pop(&self.inbound)
    }

    fn has_more_buffers(&mut self) -> bool {
        !queue_is_empty(&self.inbound)
    }

    fn put_empty_buffer(&mut self, mut buffer: Self::Buffer) {
        buffer.tags.clear();
        if let Some((ref notifier, ref buffers)) = self.circuit_start {
            queue_push(buffers, Some(buffer));
            notifier.notify();
        } else {
            warn!("Put empty buffer in unconnected circuit reader. Dropping buffer.")
        }
    }

    fn notify_consumed_buffer(&mut self) {
        if let Some((ref notifier, ref buffers)) = self.circuit_start {
            queue_push(buffers, None);
            notifier.notify();
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
            match queue_pop(&self.inbound) {
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
            match self.circuit_start {
                Some((ref notifier, ref queue)) => {
                    queue_push(queue, Some(b));
                    notifier.notify();
                }
                None => {
                    warn!(
                        "circuit reader used as cpu buffer reader but not connected to circuit start. dropping buffer."
                    );
                }
            }

            // make sure to be called again, if we have another buffer queued
            if !queue_is_empty(&self.inbound) {
                self.notifier.notify();
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
