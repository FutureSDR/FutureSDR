use std::any::Any;
use std::fmt;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::BlockInbox;
use futuresdr::runtime::BlockMessage;
use futuresdr::runtime::BlockNotifier;
use futuresdr::runtime::Error;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::PortId;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::Tags;
use futuresdr::tracing::warn;
use once_cell::sync::Lazy;
use vmcircbuffer::double_mapped_buffer::DoubleMappedBuffer;
use vmcircbuffer::double_mapped_buffer::pagesize;

static EMPTY_TAGS: Lazy<Vec<ItemTag>> = Lazy::new(Vec::new);

#[repr(align(128))]
struct PaddedAtomicUsize(AtomicUsize);

impl PaddedAtomicUsize {
    #[inline(always)]
    fn new(value: usize) -> Self {
        Self(AtomicUsize::new(value))
    }

    #[inline(always)]
    fn load(&self, ordering: Ordering) -> usize {
        self.0.load(ordering)
    }

    #[inline(always)]
    fn store(&self, value: usize, ordering: Ordering) {
        self.0.store(value, ordering);
    }
}

struct Inner<T> {
    buffer: DoubleMappedBuffer<T>,
    capacity: usize,
    write_pos: PaddedAtomicUsize,
    read_pos: PaddedAtomicUsize,
}

impl<T> Inner<T> {
    #[inline(always)]
    fn occupancy(read_pos: usize, write_pos: usize) -> usize {
        write_pos.wrapping_sub(read_pos)
    }

    #[inline(always)]
    fn space(capacity: usize, read_pos: usize, write_pos: usize) -> usize {
        capacity - Self::occupancy(read_pos, write_pos)
    }
}

pub struct Writer<T>
where
    T: CpuSample,
{
    inbox: BlockInbox,
    block_id: BlockId,
    port_id: PortId,
    inner: Option<Arc<Inner<T>>>,
    connected: bool,
    reader_inbox: BlockInbox,
    reader_input_id: PortId,
    reader_notifier: BlockNotifier,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
    last_space: usize,
    write_pos: usize,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inbox: BlockInbox::default(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inner: None,
            connected: false,
            reader_inbox: BlockInbox::default(),
            reader_input_id: PortId::default(),
            reader_notifier: BlockNotifier::new(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
            last_space: 0,
            write_pos: 0,
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }

    #[inline(always)]
    fn slice_parts(&mut self) -> &mut [T] {
        let inner = self.inner.as_ref().expect("writer not connected");
        let read_pos = inner.read_pos.load(Ordering::Acquire);
        let space = Inner::<T>::space(inner.capacity, read_pos, self.write_pos);
        debug_assert!(space <= inner.capacity);
        self.last_space = space;

        let offset = self.write_pos % inner.capacity;
        unsafe { &mut inner.buffer.slice_with_offset_mut(offset)[..space] }
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

impl<T> fmt::Debug for Writer<T>
where
    T: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("perf::spsc::Writer")
            .field("port_id", &self.port_id)
            .field("connected", &self.connected)
            .finish()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.notifier = inbox.notifier();
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.connected {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        assert!(!self.connected, "perf::spsc only supports one reader");

        let page_size = pagesize();
        let mut buffer_size = page_size;

        let min_self = self.min_items.unwrap_or(1);
        let min_reader = dest.min_items.unwrap_or(1);
        let mut min_bytes = (min_self + min_reader - 1) * size_of::<T>();

        let buffer_size_configured =
            self.min_buffer_size_in_items.is_some() || dest.min_buffer_size_in_items.is_some();

        min_bytes = if buffer_size_configured {
            let min_self = self.min_buffer_size_in_items.unwrap_or(0);
            let min_reader = dest.min_buffer_size_in_items.unwrap_or(0);
            std::cmp::max(
                min_bytes,
                std::cmp::max(min_self, min_reader) * size_of::<T>(),
            )
        } else {
            std::cmp::max(min_bytes, futuresdr::runtime::config::config().buffer_size)
        };

        while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(size_of::<T>()) {
            buffer_size += page_size;
        }

        let buffer = DoubleMappedBuffer::new(buffer_size / size_of::<T>())
            .expect("failed to allocate SPSC buffer");
        let capacity = buffer.capacity();
        let inner = Arc::new(Inner {
            buffer,
            capacity,
            write_pos: PaddedAtomicUsize::new(0),
            read_pos: PaddedAtomicUsize::new(0),
        });

        self.min_buffer_size_in_items = Some(capacity);
        dest.min_buffer_size_in_items = Some(capacity);
        self.reader_inbox = dest.inbox.clone();
        self.reader_input_id = dest.port_id.clone();
        self.reader_notifier = dest.notifier.clone();
        self.inner = Some(inner.clone());
        self.connected = true;
        self.write_pos = 0;

        dest.inner = Some(inner);
        dest.writer_inbox = self.inbox.clone();
        dest.writer_output_id = self.port_id.clone();
        dest.writer_notifier = self.notifier.clone();
        dest.read_pos = 0;
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        self.slice_parts()
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        let tags = &mut self.tags as *mut Vec<ItemTag>;
        let inner = self.inner.as_ref().expect("writer not connected");
        let read_pos = inner.read_pos.load(Ordering::Acquire);
        let space = Inner::<T>::space(inner.capacity, read_pos, self.write_pos);
        debug_assert!(space <= inner.capacity);
        self.last_space = space;

        let offset = self.write_pos % inner.capacity;
        unsafe {
            let slice = &mut inner.buffer.slice_with_offset_mut(offset)[..space];
            (slice, Tags::new(&mut *tags, 0))
        }
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            self.tags.clear();
            return;
        }

        let inner = self.inner.as_ref().expect("writer not connected");
        assert!(n <= self.last_space, "perf::spsc produced too much");

        let read_pos = inner.read_pos.load(Ordering::Acquire);
        debug_assert!(Inner::<T>::space(inner.capacity, read_pos, self.write_pos) >= n);
        self.write_pos = self.write_pos.wrapping_add(n);

        inner.write_pos.store(self.write_pos, Ordering::Release);
        self.last_space -= n;
        self.tags.clear();
        self.reader_notifier.notify();
    }

    fn set_min_items(&mut self, n: usize) {
        if self.connected {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_items = Some(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.connected {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_buffer_size_in_items = Some(n);
    }

    fn max_items(&self) -> usize {
        self.inner
            .as_ref()
            .map(|inner| inner.capacity)
            .or(self.min_buffer_size_in_items)
            .unwrap_or(usize::MAX)
    }
}

pub struct Reader<T>
where
    T: CpuSample,
{
    inner: Option<Arc<Inner<T>>>,
    finished: bool,
    writer_inbox: BlockInbox,
    writer_output_id: PortId,
    writer_notifier: BlockNotifier,
    block_id: BlockId,
    port_id: PortId,
    inbox: BlockInbox,
    notifier: BlockNotifier,
    last_space: usize,
    read_pos: usize,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inner: None,
            finished: false,
            writer_inbox: BlockInbox::default(),
            writer_output_id: PortId::default(),
            writer_notifier: BlockNotifier::new(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inbox: BlockInbox::default(),
            notifier: BlockNotifier::new(),
            last_space: 0,
            read_pos: 0,
            min_items: None,
            min_buffer_size_in_items: None,
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

impl<T> fmt::Debug for Reader<T>
where
    T: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("perf::spsc::Reader")
            .field("port_id", &self.port_id)
            .field("finished", &self.finished)
            .finish()
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
        self.block_id = block_id;
        self.port_id = port_id;
        self.notifier = inbox.notifier();
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.inner.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id.clone(),
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
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        let inner = self.inner.as_ref().expect("reader not connected");
        let write_pos = inner.write_pos.load(Ordering::Acquire);
        let avail = Inner::<T>::occupancy(self.read_pos, write_pos);
        debug_assert!(avail <= inner.capacity);
        self.last_space = avail;

        let offset = self.read_pos % inner.capacity;
        unsafe { &inner.buffer.slice_with_offset(offset)[..avail] }
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        (self.slice(), &EMPTY_TAGS)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let inner = self.inner.as_ref().expect("reader not connected");
        assert!(n <= self.last_space, "perf::spsc consumed too much");

        let write_pos = inner.write_pos.load(Ordering::Acquire);
        debug_assert!(Inner::<T>::occupancy(self.read_pos, write_pos) >= n);
        self.read_pos = self.read_pos.wrapping_add(n);

        inner.read_pos.store(self.read_pos, Ordering::Release);
        self.last_space -= n;
        self.writer_notifier.notify();
    }

    fn set_min_items(&mut self, n: usize) {
        if !self.writer_inbox.is_closed() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_items = Some(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if !self.writer_inbox.is_closed() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_buffer_size_in_items = Some(n);
    }

    fn max_items(&self) -> usize {
        self.inner
            .as_ref()
            .map(|inner| inner.capacity)
            .or(self.min_buffer_size_in_items)
            .unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_transfer() {
        let mut w = Writer::<u32>::default();
        let mut r = Reader::<u32>::default();
        w.connect(&mut r);

        let out = w.slice();
        out[..4].copy_from_slice(&[1, 2, 3, 4]);
        w.produce(4);

        let input = r.slice();
        assert_eq!(&input[..4], &[1, 2, 3, 4]);
        r.consume(4);
        assert!(r.slice().is_empty());
    }

    #[test]
    fn wraparound() {
        let mut w = Writer::<u32>::default();
        let mut r = Reader::<u32>::default();
        w.connect(&mut r);

        let cap = w.inner.as_ref().unwrap().capacity;
        {
            let out = w.slice();
            for (i, item) in out[..cap - 1].iter_mut().enumerate() {
                *item = i as u32;
            }
        }
        w.produce(cap - 1);

        let input = r.slice();
        assert_eq!(input.len(), cap - 1);
        r.consume(cap - 1);

        {
            let out = w.slice();
            out[..4].copy_from_slice(&[11, 12, 13, 14]);
        }
        w.produce(4);

        let input = r.slice();
        assert_eq!(&input[..4], &[11, 12, 13, 14]);
    }

    #[test]
    fn zero_length_ops_are_noops() {
        let mut w = Writer::<u32>::default();
        let mut r = Reader::<u32>::default();
        w.connect(&mut r);

        let _ = w.slice_with_tags().1;
        w.produce(0);
        assert!(r.slice_with_tags().1.is_empty());
        r.consume(0);
    }

    #[test]
    #[should_panic(expected = "only supports one reader")]
    fn second_reader_panics() {
        let mut w = Writer::<u32>::default();
        let mut r0 = Reader::<u32>::default();
        let mut r1 = Reader::<u32>::default();
        w.connect(&mut r0);
        w.connect(&mut r1);
    }
}
