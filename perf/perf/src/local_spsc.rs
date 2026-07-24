use std::cell::Cell;
use std::fmt;
use std::ptr;
use std::rc::Rc;
use std::slice;

use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BufferInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::LocalBlockInbox;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::PortEndpoint;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::runtime::dev::LocalBlockNotifier;
use futuresdr::tracing::warn;
use once_cell::sync::Lazy;
use vmcircbuffer::double_mapped_buffer::DoubleMappedBuffer;
use vmcircbuffer::double_mapped_buffer::pagesize;

static EMPTY_TAGS: Lazy<Vec<ItemTag>> = Lazy::new(Vec::new);

struct Inner<T> {
    _buffer: DoubleMappedBuffer<T>,
    base: *mut T,
    capacity: usize,
    write_pos: Cell<usize>,
    read_pos: Cell<usize>,
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
    core: PortCore<LocalBlockInbox>,
    inner: *const Inner<T>,
    inner_owner: Option<Rc<Inner<T>>>,
    connected: bool,
    reader: Option<PortEndpoint<LocalBlockInbox>>,
    reader_notifier: LocalBlockNotifier,
    notifier: LocalBlockNotifier,
    discarded_tags: Vec<ItemTag>,
    last_space: usize,
    write_pos: usize,
    write_offset: usize,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            inner: ptr::null(),
            inner_owner: None,
            connected: false,
            reader: None,
            reader_notifier: LocalBlockNotifier::default(),
            notifier: LocalBlockNotifier::default(),
            discarded_tags: Vec::new(),
            last_space: 0,
            write_pos: 0,
            write_offset: 0,
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }

    #[inline(always)]
    fn slice_parts(&mut self) -> &mut [T] {
        debug_assert!(!self.inner.is_null(), "writer not connected");
        let inner = self.inner;
        let inner_ref = unsafe { &*inner };
        let capacity = inner_ref.capacity;
        let read_pos = inner_ref.read_pos.get();
        let space = Inner::<T>::space(capacity, read_pos, self.write_pos);
        debug_assert!(space <= capacity);
        let offset = self.write_offset;
        self.last_space = space;

        unsafe { slice::from_raw_parts_mut(inner_ref.base.add(offset), space) }
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
        f.debug_struct("perf::local_spsc::Writer")
            .field("port_id", &self.core.port_id_if_bound())
            .field("connected", &self.connected)
            .finish()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Inbox = LocalBlockInbox;
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: LocalBlockInbox) {
        self.notifier = inbox.notifier();
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.inner_owner.is_some() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        assert!(!self.connected, "perf::local_spsc only supports one reader");

        let page_size = pagesize();
        let mut buffer_size = page_size;

        let min_self = self.min_items.unwrap_or(1);
        let min_reader = dest.min_items.unwrap_or(1);
        let mut min_bytes = (min_self + min_reader - 1) * T::SIZE.get();

        let buffer_size_configured =
            self.min_buffer_size_in_items.is_some() || dest.min_buffer_size_in_items.is_some();

        min_bytes = if buffer_size_configured {
            let min_self = self.min_buffer_size_in_items.unwrap_or(0);
            let min_reader = dest.min_buffer_size_in_items.unwrap_or(0);
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

        let buffer: DoubleMappedBuffer<T> = DoubleMappedBuffer::new(buffer_size / T::SIZE.get())
            .expect("failed to allocate SPSC buffer");
        let capacity = buffer.capacity();
        let base = unsafe { buffer.slice().as_ptr().cast_mut() };
        let inner = Rc::new(Inner {
            _buffer: buffer,
            base,
            capacity,
            write_pos: Cell::new(0),
            read_pos: Cell::new(0),
        });

        self.min_buffer_size_in_items = Some(capacity);
        dest.min_buffer_size_in_items = Some(capacity);
        self.reader = dest.core.endpoint_if_bound();
        self.reader_notifier = dest.notifier.clone();
        self.inner = Rc::as_ptr(&inner);
        self.inner_owner = Some(inner.clone());
        self.connected = true;
        self.write_pos = 0;
        self.write_offset = 0;

        dest.inner = Rc::as_ptr(&inner);
        dest.inner_owner = Some(inner);
        dest.writer = self.core.endpoint_if_bound();
        dest.writer_notifier = self.notifier.clone();
        dest.read_pos = 0;
        dest.read_offset = 0;
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

impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        self.slice_parts()
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        self.discarded_tags.clear();
        let tags = &mut self.discarded_tags as *mut Vec<ItemTag>;
        let slice = self.slice_parts();
        unsafe { (slice, Tags::new(&mut *tags, 0)) }
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            self.discarded_tags.clear();
            return;
        }

        debug_assert!(!self.inner.is_null(), "writer not connected");
        let inner = self.inner;
        let inner_ref = unsafe { &*inner };
        assert!(n <= self.last_space, "perf::local_spsc produced too much");

        let capacity = inner_ref.capacity;
        let read_pos = inner_ref.read_pos.get();
        debug_assert!(Inner::<T>::space(capacity, read_pos, self.write_pos) >= n);
        self.write_pos = self.write_pos.wrapping_add(n);

        let mut write_offset = self.write_offset + n;
        if write_offset >= capacity {
            write_offset -= capacity;
        }
        self.write_offset = write_offset;

        inner_ref.write_pos.set(self.write_pos);
        self.last_space -= n;
        self.discarded_tags.clear();
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
        self.inner_owner
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
    inner: *const Inner<T>,
    inner_owner: Option<Rc<Inner<T>>>,
    finished: bool,
    writer: Option<PortEndpoint<LocalBlockInbox>>,
    writer_notifier: LocalBlockNotifier,
    core: PortCore<LocalBlockInbox>,
    notifier: LocalBlockNotifier,
    last_space: usize,
    read_pos: usize,
    read_offset: usize,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inner: ptr::null(),
            inner_owner: None,
            finished: false,
            writer: None,
            writer_notifier: LocalBlockNotifier::default(),
            core: PortCore::new_unbound(),
            notifier: LocalBlockNotifier::default(),
            last_space: 0,
            read_pos: 0,
            read_offset: 0,
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
        f.debug_struct("perf::local_spsc::Reader")
            .field("port_id", &self.core.port_id_if_bound())
            .field("finished", &self.finished)
            .finish()
    }
}

impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
{
    type Inbox = LocalBlockInbox;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: LocalBlockInbox) {
        self.notifier = inbox.notifier();
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.inner_owner.is_some() {
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
        debug_assert!(!self.inner.is_null(), "reader not connected");
        let inner = self.inner;
        let inner_ref = unsafe { &*inner };
        let capacity = inner_ref.capacity;
        let write_pos = inner_ref.write_pos.get();
        let avail = Inner::<T>::occupancy(self.read_pos, write_pos);
        debug_assert!(avail <= capacity);
        let offset = self.read_offset;
        self.last_space = avail;

        unsafe { slice::from_raw_parts(inner_ref.base.add(offset), avail) }
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        (self.slice(), &EMPTY_TAGS)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        debug_assert!(!self.inner.is_null(), "reader not connected");
        let inner = self.inner;
        let inner_ref = unsafe { &*inner };
        assert!(n <= self.last_space, "perf::local_spsc consumed too much");

        let capacity = inner_ref.capacity;
        let write_pos = inner_ref.write_pos.get();
        debug_assert!(Inner::<T>::occupancy(self.read_pos, write_pos) >= n);
        self.read_pos = self.read_pos.wrapping_add(n);

        let mut read_offset = self.read_offset + n;
        if read_offset >= capacity {
            read_offset -= capacity;
        }
        self.read_offset = read_offset;

        inner_ref.read_pos.set(self.read_pos);
        self.last_space -= n;
        self.writer_notifier.notify();
    }

    fn set_min_items(&mut self, n: usize) {
        if self.inner_owner.is_some() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_items = Some(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.inner_owner.is_some() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_buffer_size_in_items = Some(n);
    }

    fn max_items(&self) -> usize {
        self.inner_owner
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

        let cap = w.inner_owner.as_ref().unwrap().capacity;
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
