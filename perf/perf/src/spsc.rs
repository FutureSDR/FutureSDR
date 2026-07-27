use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BlockInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferRequirements;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::PortEndpoint;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::dev::BlockNotifier;
use futuresdr::runtime::dev::ItemTag;
use vmcircbuffer::double_mapped_buffer::DoubleMappedBuffer;
use vmcircbuffer::double_mapped_buffer::pagesize;

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

pub struct ThreadSafeConnectToken<T>
where
    T: CpuSample,
{
    reader: PortEndpoint,
    reader_notifier: BlockNotifier,
    reader_min_items: Option<usize>,
    reader_min_buffer_size_in_items: Option<usize>,
    _item: std::marker::PhantomData<T>,
}

pub struct ThreadSafeReturnToken<T>
where
    T: CpuSample,
{
    inner: Arc<Inner<T>>,
    writer: PortEndpoint,
    writer_notifier: BlockNotifier,
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
    core: PortCore,
    inner: Option<Arc<Inner<T>>>,
    connected: bool,
    reader: Option<PortEndpoint>,
    reader_notifier: BlockNotifier,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
    last_space: usize,
    write_pos: usize,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            inner: None,
            connected: false,
            reader: None,
            reader_notifier: BlockNotifier::new(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
            last_space: 0,
            write_pos: 0,
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
            .field("port_id", &self.core.port_id_if_bound())
            .field("connected", &self.connected)
            .finish()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Inbox = BlockInbox;
    type Reader = Reader<T>;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.notifier = inbox.notifier();
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.connected {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        assert!(!self.connected, "perf::spsc only supports one reader");

        let page_size = pagesize();
        let mut buffer_size = page_size;

        let min_self = self.core.min_items().unwrap_or(1);
        let min_reader = dest.core.min_items().unwrap_or(1);
        let mut min_bytes = (min_self + min_reader - 1) * T::SIZE.get();

        let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
            || dest.core.min_buffer_size_in_items().is_some();

        min_bytes = if buffer_size_configured {
            let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
            let min_reader = dest.core.min_buffer_size_in_items().unwrap_or(0);
            std::cmp::max(
                min_bytes,
                std::cmp::max(min_self, min_reader) * T::SIZE.get(),
            )
        } else {
            std::cmp::max(min_bytes, futuresdr::runtime::config::config().buffer_size)
        };

        while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(T::SIZE.get()) {
            buffer_size += page_size;
        }

        let buffer = DoubleMappedBuffer::new(buffer_size / T::SIZE.get())
            .expect("failed to allocate SPSC buffer");
        let capacity = buffer.capacity();
        let inner = Arc::new(Inner {
            buffer,
            capacity,
            write_pos: PaddedAtomicUsize::new(0),
            read_pos: PaddedAtomicUsize::new(0),
        });

        self.core.set_min_buffer_size_in_items(capacity);
        dest.core.set_min_buffer_size_in_items(capacity);
        self.reader = dest.core.endpoint_if_bound();
        self.reader_notifier = dest.notifier.clone();
        self.inner = Some(inner.clone());
        self.connected = true;
        self.write_pos = 0;

        dest.inner = Some(inner);
        dest.writer = self.core.endpoint_if_bound();
        dest.writer_notifier = self.notifier.clone();
        dest.read_pos = 0;
    }

    async fn notify_finished(&mut self) {
        if let Some(reader) = &self.reader {
            let _ = reader.inbox().stream_input_done(reader.port_id()).await;
        }
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<T> ThreadSafeConnect for Writer<T>
where
    T: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<T>;
    type WriterToken = ThreadSafeReturnToken<T>;

    fn take_reader_token(reader: &mut Reader<T>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: reader
                .core
                .endpoint_if_bound()
                .expect("reader port not bound to a flowgraph"),
            reader_notifier: reader.notifier.clone(),
            reader_min_items: reader.core.min_items(),
            reader_min_buffer_size_in_items: reader.core.min_buffer_size_in_items(),
            _item: std::marker::PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        assert!(!self.connected, "perf::spsc only supports one reader");
        let page_size = pagesize();
        let mut buffer_size = page_size;

        let min_self = self.core.min_items().unwrap_or(1);
        let min_reader = token.reader_min_items.unwrap_or(1);
        let mut min_bytes = (min_self + min_reader - 1) * T::SIZE.get();

        let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
            || token.reader_min_buffer_size_in_items.is_some();
        min_bytes = if buffer_size_configured {
            std::cmp::max(
                min_bytes,
                std::cmp::max(
                    self.core.min_buffer_size_in_items().unwrap_or(0),
                    token.reader_min_buffer_size_in_items.unwrap_or(0),
                ) * T::SIZE.get(),
            )
        } else {
            std::cmp::max(min_bytes, futuresdr::runtime::config::config().buffer_size)
        };

        while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(T::SIZE.get()) {
            buffer_size += page_size;
        }

        let buffer = DoubleMappedBuffer::new(buffer_size / T::SIZE.get())
            .expect("failed to allocate SPSC buffer");
        let capacity = buffer.capacity();
        let inner = Arc::new(Inner {
            buffer,
            capacity,
            write_pos: PaddedAtomicUsize::new(0),
            read_pos: PaddedAtomicUsize::new(0),
        });

        self.core.set_min_buffer_size_in_items(capacity);
        self.reader = Some(token.reader);
        self.reader_notifier = token.reader_notifier;
        self.inner = Some(inner.clone());
        self.connected = true;
        self.write_pos = 0;

        ThreadSafeReturnToken {
            inner,
            writer: self
                .core
                .endpoint_if_bound()
                .expect("writer port not bound to a flowgraph"),
            writer_notifier: self.notifier.clone(),
        }
    }

    fn finish_reader(reader: &mut Reader<T>, token: Self::WriterToken) {
        reader
            .core
            .set_min_buffer_size_in_items(token.inner.capacity);
        reader.inner = Some(token.inner);
        reader.writer = Some(token.writer);
        reader.writer_notifier = token.writer_notifier;
        reader.read_pos = 0;
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

        debug_assert!(
            Inner::<T>::space(
                inner.capacity,
                inner.read_pos.load(Ordering::Acquire),
                self.write_pos
            ) >= n
        );
        self.write_pos = self.write_pos.wrapping_add(n);

        inner.write_pos.store(self.write_pos, Ordering::Release);
        self.last_space -= n;
        self.tags.clear();
        self.reader_notifier.notify();
    }
}

pub struct Reader<T>
where
    T: CpuSample,
{
    inner: Option<Arc<Inner<T>>>,
    finished: bool,
    writer: Option<PortEndpoint>,
    writer_notifier: BlockNotifier,
    core: PortCore,
    notifier: BlockNotifier,
    last_space: usize,
    read_pos: usize,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inner: None,
            finished: false,
            writer: None,
            writer_notifier: BlockNotifier::new(),
            core: PortCore::new_unbound(),
            notifier: BlockNotifier::new(),
            last_space: 0,
            read_pos: 0,
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
            .field("port_id", &self.core.port_id_if_bound())
            .field("finished", &self.finished)
            .finish()
    }
}

impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
{
    type Inbox = BlockInbox;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.notifier = inbox.notifier();
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.inner.is_some() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    async fn notify_finished(&mut self) {
        if let Some(writer) = &self.writer {
            let _ = writer.inbox().stream_output_done(writer.port_id()).await;
        }
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

    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        (self.slice(), &[])
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let inner = self.inner.as_ref().expect("reader not connected");
        assert!(n <= self.last_space, "perf::spsc consumed too much");

        debug_assert!(
            Inner::<T>::occupancy(self.read_pos, inner.write_pos.load(Ordering::Acquire)) >= n
        );
        self.read_pos = self.read_pos.wrapping_add(n);

        inner.read_pos.store(self.read_pos, Ordering::Release);
        self.last_space -= n;
        self.writer_notifier.notify();
    }

    fn max_contiguous_items(&self) -> usize {
        self.inner
            .as_ref()
            .map(|inner| inner.capacity)
            .or(self.core.min_buffer_size_in_items())
            .expect("SPSC buffer capacity missing after validation")
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
