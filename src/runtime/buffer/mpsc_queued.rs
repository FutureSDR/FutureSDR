//! Internal implementation shared only by the two fanout queue families.
#![allow(missing_docs)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use wasm_spin::Mutex;

#[cfg(target_arch = "wasm32")]
mod wasm_spin {
    use std::cell::UnsafeCell;
    use std::fmt;
    use std::hint::spin_loop;
    use std::ops::Deref;
    use std::ops::DerefMut;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    pub(super) struct Mutex<T> {
        locked: AtomicBool,
        value: UnsafeCell<T>,
    }

    unsafe impl<T: Send> Send for Mutex<T> {}
    unsafe impl<T: Send> Sync for Mutex<T> {}

    impl<T> Mutex<T> {
        pub(super) fn new(value: T) -> Self {
            Self {
                locked: AtomicBool::new(false),
                value: UnsafeCell::new(value),
            }
        }

        pub(super) fn lock(&self) -> MutexGuard<'_, T> {
            while self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                spin_loop();
            }

            MutexGuard { mutex: self }
        }
    }

    impl<T> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Mutex").finish_non_exhaustive()
        }
    }

    pub(super) struct MutexGuard<'a, T> {
        mutex: &'a Mutex<T>,
    }

    impl<T> Deref for MutexGuard<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.mutex.value.get() }
        }
    }

    impl<T> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.mutex.value.get() }
        }
    }

    impl<T> Drop for MutexGuard<'_, T> {
        fn drop(&mut self) {
            self.mutex.locked.store(false, Ordering::Release);
        }
    }
}

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CacheAlignedBuffer;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::ThreadSafeConnect;
use crate::runtime::config;
use crate::runtime::dev::ItemTag;

const DEFAULT_BUFFER_COUNT: usize = 2;

#[derive(Debug)]
pub struct Page<D: CpuSample> {
    buffer: CacheAlignedBuffer<D>,
    prefix: usize,
    valid_start: usize,
    end: usize,
    tags: Vec<ItemTag>,
}

// Each lease owns one reference, either in a reader queue or in a reader's
// current slice. The pool owns a separate reference while a page is published.
#[derive(Debug)]
struct Lease<D: CpuSample, S: SharedState<D>> {
    slot: usize,
    page: S::Page,
    _item: PhantomData<D>,
}

impl<D: CpuSample, S: SharedState<D>> Clone for Lease<D, S> {
    fn clone(&self) -> Self {
        Self {
            slot: self.slot,
            page: self.page.clone(),
            _item: PhantomData,
        }
    }
}

#[derive(Debug)]
struct Subscriber<D: CpuSample, S: SharedState<D>> {
    active: bool,
    queue: VecDeque<Lease<D, S>>,
}

#[derive(Debug)]
pub struct State<D: CpuSample, S: SharedState<D>> {
    requirements: BufferRequirements,
    reserved: usize,
    min_buffers: usize,
    started: bool,
    free: VecDeque<Lease<D, S>>,
    published: Vec<Option<S::Page>>,
    readers: Vec<Subscriber<D, S>>,
}

impl<D: CpuSample, S: SharedState<D>> State<D, S> {
    fn page_items(&self) -> usize {
        self.requirements
            .min_buffer_size_in_items()
            .unwrap_or_else(|| config::config().buffer_size / D::SIZE)
            .max(self.requirements.min_items().unwrap_or(1))
            .max(1)
    }

    fn initialize(&mut self) {
        if self.started {
            return;
        }
        let count = self.min_buffers.max(if self.reserved > 0 { 2 } else { 1 });
        let capacity = self
            .reserved
            .checked_add(self.page_items())
            .expect("queue capacity overflow");
        for slot in 0..count {
            self.published.push(None);
            self.free.push_back(Lease {
                slot,
                page: S::new_page(Page {
                    buffer: CacheAlignedBuffer::new(capacity),
                    prefix: self.reserved,
                    valid_start: self.reserved,
                    end: self.reserved,
                    tags: Vec::new(),
                }),
                _item: PhantomData,
            });
        }
        self.started = true;
    }

    fn recycle(&mut self, slot: usize) -> bool {
        if self.published[slot]
            .as_ref()
            .is_some_and(|page| S::page_refs(page) == 1)
        {
            let page = self.published[slot].take().unwrap();
            self.free.push_back(Lease {
                slot,
                page,
                _item: PhantomData,
            });
            true
        } else {
            false
        }
    }

    fn release(&mut self, lease: Lease<D, S>) -> bool {
        let slot = lease.slot;
        // Reference release and the pool's uniqueness check are serialized.
        // All clones originate in publish(), also under this same lock/borrow.
        drop(lease);
        self.recycle(slot)
    }

    fn publish(&mut self, lease: Lease<D, S>) {
        for reader in &mut self.readers {
            if reader.active {
                reader.queue.push_back(lease.clone());
            }
        }
        let slot = lease.slot;
        debug_assert!(self.published[slot].is_none());
        self.published[slot] = Some(lease.page);
        self.recycle(slot);
    }
}

pub trait SharedState<D: CpuSample>: Clone + Debug + Sized + 'static {
    type Page: Clone + Debug + Deref<Target = Page<D>>;
    fn new(state: State<D, Self>) -> Self;
    fn with<R>(&self, f: impl FnOnce(&mut State<D, Self>) -> R) -> R;
    fn new_page(page: Page<D>) -> Self::Page;
    fn page_mut(page: &mut Self::Page) -> &mut Page<D>;
    fn page_refs(page: &Self::Page) -> usize;
}

#[derive(Debug)]
pub struct LocalState<D: CpuSample>(Rc<RefCell<State<D, Self>>>);
impl<D: CpuSample> Clone for LocalState<D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<D: CpuSample> SharedState<D> for LocalState<D> {
    type Page = Rc<Page<D>>;
    fn new(state: State<D, Self>) -> Self {
        Self(Rc::new(RefCell::new(state)))
    }
    fn with<R>(&self, f: impl FnOnce(&mut State<D, Self>) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }
    fn new_page(page: Page<D>) -> Self::Page {
        Rc::new(page)
    }
    fn page_mut(page: &mut Self::Page) -> &mut Page<D> {
        Rc::get_mut(page).expect("writer must exclusively own its page")
    }
    fn page_refs(page: &Self::Page) -> usize {
        Rc::strong_count(page)
    }
}

#[derive(Debug)]
pub struct SendState<D: CpuSample>(Arc<Mutex<State<D, Self>>>);
impl<D: CpuSample> Clone for SendState<D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<D: CpuSample> SharedState<D> for SendState<D> {
    type Page = Arc<Page<D>>;
    fn new(state: State<D, Self>) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn with<R>(&self, f: impl FnOnce(&mut State<D, Self>) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }
    #[cfg(target_arch = "wasm32")]
    fn with<R>(&self, f: impl FnOnce(&mut State<D, Self>) -> R) -> R {
        f(&mut self.0.lock())
    }
    fn new_page(page: Page<D>) -> Self::Page {
        Arc::new(page)
    }
    fn page_mut(page: &mut Self::Page) -> &mut Page<D> {
        Arc::get_mut(page).expect("writer must exclusively own its page")
    }
    fn page_refs(page: &Self::Page) -> usize {
        Arc::strong_count(page)
    }
}

#[derive(Debug)]
struct History<D: CpuSample> {
    samples: CacheAlignedBuffer<D>,
    valid: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct ConnectedWriter<D: CpuSample, S: SharedState<D>, I: BufferInbox> {
    state: S,
    readers: Vec<PortEndpoint<I>>,
    _item: PhantomData<D>,
}

#[derive(Debug)]
struct ConnectedReader<D: CpuSample, S: SharedState<D>, I: BufferInbox> {
    state: S,
    reader: usize,
    reserved: usize,
    writer: PortEndpoint<I>,
    _item: PhantomData<D>,
}

/// Bounded queue writer with shared immutable pages for fanout.
#[derive(Debug)]
pub struct Writer<D: CpuSample, S: SharedState<D>, I: BufferInbox = BlockInbox> {
    core: PortCore<I>,
    state: ConnectionState<ConnectedWriter<D, S, I>>,
    min_buffers: usize,
    current: Option<Lease<D, S>>,
    history: Option<History<D>>,
    tags: Vec<ItemTag>,
}

impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> Writer<D, S, I> {
    /// Create an unconnected writer.
    pub fn new() -> Self {
        Self {
            core: PortCore::with_requirements(BufferRequirements::with_min_items(1)),
            state: ConnectionState::disconnected(),
            min_buffers: DEFAULT_BUFFER_COUNT,
            current: None,
            history: None,
            tags: Vec::new(),
        }
    }

    /// Set the minimum pool size before connecting. Overlap needs at least two pages.
    pub fn set_min_buffers(&mut self, count: usize) {
        assert!(count > 0, "a queue needs at least one page");
        if self.state.is_connected() {
            warn!("buffer count configured after connection; ignoring it");
            return;
        }
        self.min_buffers = count;
    }

    fn add_reader(
        &mut self,
        reader: PortEndpoint<I>,
        requirements: BufferRequirements,
        min_buffers: usize,
    ) -> ConnectedReader<D, S, I> {
        if !self.state.is_connected() {
            self.state.set_connected(ConnectedWriter {
                state: S::new(State {
                    requirements: self.core.requirements(),
                    reserved: 0,
                    min_buffers: self.min_buffers,
                    started: false,
                    free: VecDeque::new(),
                    published: Vec::new(),
                    readers: Vec::new(),
                }),
                readers: Vec::new(),
                _item: PhantomData,
            });
        }
        let connected = self.state.connected_mut();
        let reserved = requirements.min_items().unwrap_or(0);
        let id = connected.state.with(|state| {
            assert!(
                !state.started,
                "readers must connect before streaming starts"
            );
            state.requirements.merge(self.core.requirements());
            state.requirements.merge(requirements);
            state.reserved = state.reserved.max(reserved);
            state.min_buffers = state.min_buffers.max(min_buffers);
            let id = state.readers.len();
            state.readers.push(Subscriber {
                active: true,
                queue: VecDeque::new(),
            });
            id
        });
        connected.readers.push(reader);
        ConnectedReader {
            state: connected.state.clone(),
            reader: id,
            reserved,
            writer: PortEndpoint::new(self.core.inbox().clone(), self.core.port_id()),
            _item: PhantomData,
        }
    }

    fn publish(&mut self) {
        let lease = self.current.take().unwrap();
        let page = &*lease.page;
        let history = self.history.as_mut().unwrap();
        history.valid = page.prefix.min(page.end - page.valid_start);
        let start = page.end - history.valid;
        history.samples[..history.valid].copy_from_slice(&page.buffer[start..page.end]);
        history.tags.clear();
        history.tags.extend(page.tags.iter().filter_map(|tag| {
            if tag.index >= start && tag.index < page.end {
                let mut tag = tag.clone();
                tag.index -= start;
                Some(tag)
            } else {
                None
            }
        }));
        let connected = self.state.connected();
        connected.state.with(|state| state.publish(lease));
        for reader in &connected.readers {
            reader.inbox().notify();
        }
        if connected.state.with(|state| !state.free.is_empty()) {
            self.core.inbox().notify();
        }
    }
}

impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> Default for Writer<D, S, I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> BufferWriter for Writer<D, S, I> {
    type Inbox = I;
    type Reader = Reader<D, S, I>;
    fn max_readers(&self) -> usize {
        usize::MAX
    }
    fn init(&mut self, block_id: BlockId, port: PortIndex, inbox: I) {
        self.core.init(block_id, port, inbox);
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
    fn connect(&mut self, reader: &mut Self::Reader) {
        assert!(!reader.state.is_connected(), "reader is already connected");
        let connected = self.add_reader(
            PortEndpoint::new(reader.core.inbox().clone(), reader.core.port_id()),
            reader.core.requirements(),
            reader.min_buffers,
        );
        reader.state.set_connected(connected);
    }
    async fn notify_finished(&mut self) {
        if self
            .current
            .as_ref()
            .is_some_and(|lease| lease.page.end > lease.page.prefix)
        {
            self.publish();
        }
        for reader in &self.state.connected().readers {
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

pub struct ReaderToken {
    reader: PortEndpoint<BlockInbox>,
    requirements: BufferRequirements,
    min_buffers: usize,
}
pub struct WriterToken<D: CpuSample, S: SharedState<D>> {
    connected: ConnectedReader<D, S, BlockInbox>,
}

impl<D: CpuSample, S: SharedState<D> + Send> ThreadSafeConnect for Writer<D, S, BlockInbox> {
    type ReaderToken = ReaderToken;
    type WriterToken = WriterToken<D, S>;
    fn take_reader_token(reader: &mut Self::Reader) -> Self::ReaderToken {
        assert!(!reader.state.is_connected(), "reader is already connected");
        ReaderToken {
            reader: PortEndpoint::new(reader.core.inbox().clone(), reader.core.port_id()),
            requirements: reader.core.requirements(),
            min_buffers: reader.min_buffers,
        }
    }
    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        WriterToken {
            connected: self.add_reader(token.reader, token.requirements, token.min_buffers),
        }
    }
    fn finish_reader(reader: &mut Self::Reader, token: Self::WriterToken) {
        reader.state.set_connected(token.connected);
    }
}

impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> CpuBufferWriter for Writer<D, S, I> {
    type Item = D;
    fn slice_with_tags(&mut self) -> (&mut [D], Tags<'_>) {
        if self.current.is_none() {
            let state = &self.state.connected().state;
            let (next, reserved) = state.with(|state| {
                state.initialize();
                (state.free.pop_front(), state.reserved)
            });
            let Some(mut next) = next else {
                return (&mut [], Tags::new(&mut self.tags, 0));
            };
            let history = self.history.get_or_insert_with(|| History {
                samples: CacheAlignedBuffer::new(reserved),
                valid: 0,
                tags: Vec::new(),
            });
            let page = S::page_mut(&mut next.page);
            page.valid_start = reserved - history.valid;
            page.end = reserved;
            page.buffer[page.valid_start..reserved]
                .copy_from_slice(&history.samples[..history.valid]);
            page.tags.clear();
            page.tags.extend(history.tags.iter().map(|tag| {
                let mut tag = tag.clone();
                tag.index += page.valid_start;
                tag
            }));
            self.current = Some(next);
        }
        let page = S::page_mut(&mut self.current.as_mut().unwrap().page);
        (&mut page.buffer[page.end..], Tags::new(&mut self.tags, 0))
    }
    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let page = S::page_mut(&mut self.current.as_mut().unwrap().page);
        assert!(
            n <= page.buffer.len() - page.end,
            "produced beyond writable slice"
        );
        for mut tag in self.tags.drain(..) {
            tag.index += page.end;
            page.tags.push(tag);
        }
        page.end += n;
        if page.buffer.len() - page.end < self.core.min_items().unwrap_or(1) {
            self.publish();
        }
    }
}

#[derive(Debug)]
struct CurrentReader<D: CpuSample, S: SharedState<D>> {
    lease: Lease<D, S>,
    offset: usize,
}

/// Reader with an independent position in immutable shared pages.
#[derive(Debug)]
pub struct Reader<D: CpuSample, S: SharedState<D>, I: BufferInbox = BlockInbox> {
    core: PortCore<I>,
    state: ConnectionState<ConnectedReader<D, S, I>>,
    min_buffers: usize,
    current: Option<CurrentReader<D, S>>,
    tags: Vec<ItemTag>,
    finished: bool,
}

impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> Reader<D, S, I> {
    /// Create an unconnected reader.
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            min_buffers: DEFAULT_BUFFER_COUNT,
            current: None,
            tags: Vec::new(),
            finished: false,
        }
    }
    /// Set the minimum pool size before connecting.
    pub fn set_min_buffers(&mut self, count: usize) {
        assert!(count > 0, "a queue needs at least one page");
        if self.state.is_connected() {
            warn!("buffer count configured after connection; ignoring it");
            return;
        }
        self.min_buffers = count;
    }
    fn detach(&mut self) {
        if let Some(connected) = self.state.as_ref() {
            let current = self.current.take();
            let recycled = connected.state.with(|state| {
                state.readers[connected.reader].active = false;
                let mut recycled = false;
                if let Some(current) = current {
                    recycled |= state.release(current.lease);
                }
                while let Some(lease) = state.readers[connected.reader].queue.pop_front() {
                    recycled |= state.release(lease);
                }
                recycled
            });
            if recycled {
                connected.writer.inbox().notify();
            }
        }
    }
}
impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> Default for Reader<D, S, I> {
    fn default() -> Self {
        Self::new()
    }
}
impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> Drop for Reader<D, S, I> {
    fn drop(&mut self) {
        self.detach();
    }
}
impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> BufferReader for Reader<D, S, I> {
    type Inbox = I;
    fn init(&mut self, block_id: BlockId, port: PortIndex, inbox: I) {
        self.core.init(block_id, port, inbox);
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
        self.detach();
        let writer = &self.state.connected().writer;
        let _ = writer.inbox().stream_output_done(writer.port_id()).await;
    }
    fn finish(&mut self) {
        self.finished = true;
    }
    fn finished(&self) -> bool {
        self.finished
            && self.state.as_ref().is_none_or(|connected| {
                connected
                    .state
                    .with(|state| state.readers[connected.reader].queue.is_empty())
            })
    }
    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }
    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}
impl<D: CpuSample, S: SharedState<D>, I: BufferInbox> CpuBufferReader for Reader<D, S, I> {
    type Item = D;
    fn slice_with_tags(&mut self) -> (&[D], &[ItemTag]) {
        let connected = self.state.connected();
        let left = self
            .current
            .as_ref()
            .map_or(0, |cur| cur.lease.page.end - cur.offset);
        if self.current.is_none() || left <= connected.reserved {
            let next = connected
                .state
                .with(|state| state.readers[connected.reader].queue.pop_front());
            if let Some(lease) = next {
                let offset = lease.page.prefix - left;
                assert!(offset >= lease.page.valid_start, "missing stream history");
                if let Some(old) = self.current.take()
                    && connected.state.with(|state| state.release(old.lease))
                {
                    connected.writer.inbox().notify();
                }
                self.current = Some(CurrentReader { lease, offset });
            }
        }
        let Some(current) = &self.current else {
            return (&[], &[]);
        };
        let page = &*current.lease.page;
        self.tags.clear();
        self.tags.extend(page.tags.iter().filter_map(|tag| {
            if tag.index >= current.offset && tag.index < page.end {
                let mut tag = tag.clone();
                tag.index -= current.offset;
                Some(tag)
            } else {
                None
            }
        }));
        (&page.buffer[current.offset..page.end], &self.tags)
    }
    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let connected = self.state.connected();
        let current = self.current.as_mut().unwrap();
        assert!(
            n <= current.lease.page.end - current.offset,
            "consumed beyond readable slice"
        );
        current.offset += n;
        let left = current.lease.page.end - current.offset;
        if left == 0 {
            let old = self.current.take().unwrap();
            if connected.state.with(|state| state.release(old.lease)) {
                connected.writer.inbox().notify();
            }
        }
        if left <= connected.reserved
            && connected
                .state
                .with(|state| !state.readers[connected.reader].queue.is_empty())
        {
            self.core.inbox().notify();
        }
    }
    fn max_contiguous_items(&self) -> usize {
        self.state
            .connected()
            .state
            .with(|state| state.page_items())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::runtime::block_inbox::LocalBlockInbox;
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::block_on;
    use crate::runtime::dev::Tag;

    fn local_inbox() -> LocalBlockInbox {
        LocalBlockInboxReader::pair().0
    }
    fn send_inbox() -> BlockInbox {
        BlockInbox::pair(16).0
    }

    type TestConnection<S, I> = (Writer<u32, S, I>, Vec<Reader<u32, S, I>>);

    fn setup<S: SharedState<u32>, I: BufferInbox>(
        inbox: fn() -> I,
        requirements: &[usize],
        page_items: usize,
        count: usize,
    ) -> TestConnection<S, I> {
        let mut writer = Writer::new();
        BufferWriter::init(&mut writer, BlockId(0), PortIndex::new(0), inbox());
        writer.set_min_buffers(count);
        BufferWriter::set_min_buffer_size_in_items(&mut writer, page_items);
        let readers = requirements
            .iter()
            .enumerate()
            .map(|(i, &required)| {
                let mut reader = Reader::new();
                BufferReader::init(&mut reader, BlockId(i + 1), PortIndex::new(0), inbox());
                reader.set_min_buffers(count);
                BufferReader::set_min_items(&mut reader, required);
                BufferWriter::connect(&mut writer, &mut reader);
                reader
            })
            .collect();
        (writer, readers)
    }

    fn roundtrip<S: SharedState<u32>, I: BufferInbox>(inbox: fn() -> I) {
        // Exercise reader registration in both orders, including a late reader
        // that increases the allocation prefix and usable capacity.
        for requirements in [[0, 3, 9], [9, 3, 0]] {
            let (mut writer, mut readers) = setup::<S, I>(inbox, &requirements, 7, 3);
            let total = 2003;
            let mut produced = 0usize;
            let mut consumed = [0usize; 3];
            let mut ended = false;
            for tick in 0..50_000usize {
                if !ended {
                    let (out, mut tags) = writer.slice_with_tags();
                    let n = out.len().min(1 + tick % 7).min(total - produced);
                    for (i, sample) in out[..n].iter_mut().enumerate() {
                        *sample = (produced + i) as u32;
                        tags.add_tag(i, Tag::Id((produced + i) as u64));
                    }
                    writer.produce(n);
                    produced += n;
                    if produced == total {
                        block_on(writer.notify_finished());
                        for reader in &mut readers {
                            reader.finish();
                        }
                        ended = true;
                    }
                }
                for (i, reader) in readers.iter_mut().enumerate() {
                    // A different speed for each reader makes page reclamation
                    // and overlap transitions interleave throughout the run.
                    if !tick.is_multiple_of(i * 3 + 1) {
                        continue;
                    }
                    let finished = reader.finished();
                    let (input, tags) = reader.slice_with_tags();
                    assert!(
                        input
                            .iter()
                            .copied()
                            .eq((consumed[i] as u32)..(consumed[i] + input.len()) as u32)
                    );
                    assert_eq!(tags.len(), input.len());
                    for (offset, tag) in tags.iter().enumerate() {
                        assert_eq!(tag.index, offset);
                        assert_eq!(tag.tag, Tag::Id((consumed[i] + offset) as u64));
                    }
                    let required = requirements[i].max(1);
                    let n = if input.len() >= required {
                        (1 + tick % 5).min(input.len() - required + 1)
                    } else if finished {
                        input.len()
                    } else {
                        0
                    };
                    reader.consume(n);
                    consumed[i] += n;
                }
                if consumed == [total; 3] {
                    break;
                }
                assert!(
                    tick < 49_999,
                    "stalled at produced={produced}, consumed={consumed:?}"
                );
            }
            assert_eq!(consumed, [total; 3]);
            assert!(
                writer
                    .state
                    .connected()
                    .state
                    .with(|state| state.published.iter().all(Option::is_none))
            );
        }
    }

    #[test]
    fn local_fanout_samples_and_tags() {
        roundtrip::<LocalState<u32>, _>(local_inbox);
    }
    #[test]
    fn threaded_fanout_samples_and_tags() {
        roundtrip::<SendState<u32>, _>(send_inbox);
    }

    fn retained_slice<S: SharedState<u32>, I: BufferInbox>(inbox: fn() -> I) {
        let (mut writer, mut readers) = setup::<S, I>(inbox, &[0, 0], 4, 1);
        writer.slice().copy_from_slice(&[10, 11, 12, 13]);
        writer.produce(4);
        let (slow, fast) = readers.split_at_mut(1);
        let held = slow[0].slice();
        assert_eq!(
            held.as_ptr(),
            fast[0].slice().as_ptr(),
            "payload must be shared"
        );
        fast[0].consume(4);
        assert!(writer.slice().is_empty(), "slow reader must prevent reuse");
        assert_eq!(held, &[10, 11, 12, 13]);
        slow[0].consume(4);
        assert_eq!(writer.slice().len(), 4);
    }
    #[test]
    fn local_page_is_not_recycled_while_borrowed() {
        retained_slice::<LocalState<u32>, _>(local_inbox);
    }
    #[test]
    fn threaded_page_is_not_recycled_while_borrowed() {
        retained_slice::<SendState<u32>, _>(send_inbox);
    }

    #[test]
    fn reader_drop_releases_current_and_queued_pages() {
        let (mut writer, mut readers) = setup::<LocalState<u32>, _>(local_inbox, &[0, 0], 4, 2);
        for _ in 0..2 {
            writer.slice().fill(42);
            writer.produce(4);
        }
        readers[0].slice();
        for _ in 0..2 {
            readers[1].slice();
            readers[1].consume(4);
        }
        assert!(writer.slice().is_empty());
        drop(readers.remove(0));
        assert_eq!(writer.slice().len(), 4);
        writer.produce(4);
        assert_eq!(readers[0].slice(), &[42; 4]);
        let connected = writer.state.connected();
        assert!(
            connected
                .state
                .with(|state| state.readers[0].queue.is_empty())
        );
    }

    #[test]
    fn overlap_with_one_requested_page_still_progresses() {
        let (mut writer, mut readers) = setup::<LocalState<u32>, _>(local_inbox, &[3], 4, 1);
        writer.slice().copy_from_slice(&[0, 1, 2, 3]);
        writer.produce(4);
        assert_eq!(readers[0].slice(), &[0, 1, 2, 3]);
        readers[0].consume(2);
        writer.slice().copy_from_slice(&[4, 5, 6, 7]);
        writer.produce(4);
        assert_eq!(readers[0].slice(), &[2, 3, 4, 5, 6, 7]);
        assert_eq!(writer.slice().len(), 4);
    }

    #[test]
    fn short_pages_accumulate_history_without_mutating_shared_pages() {
        let (mut writer, mut readers) = setup::<LocalState<u32>, _>(local_inbox, &[4, 1], 4, 2);
        // A writer may publish a short page because its work-unit requirement
        // leaves insufficient remaining space, not just at end of stream.
        BufferWriter::set_min_items(&mut writer, 4);
        for n in 0..12 {
            writer.slice()[0] = n;
            writer.produce(1);
            assert_eq!(readers[1].slice(), &[n]);
            readers[1].consume(1);
            let input = readers[0].slice();
            let len = input.len();
            if len >= 4 {
                assert_eq!(input, &[n - 3, n - 2, n - 1, n]);
                readers[0].consume(4);
            }
        }
    }

    #[test]
    fn overlap_transition_rearms_reader_notification() {
        let (mut writer, mut readers) = setup::<LocalState<u32>, _>(local_inbox, &[2], 4, 2);
        let (inbox, pending) = LocalBlockInboxReader::pair();
        readers[0].core.init(BlockId(1), PortIndex::new(0), inbox);
        for values in [[0, 1, 2, 3], [4, 5, 6, 7]] {
            writer.slice().copy_from_slice(&values);
            writer.produce(4);
        }
        readers[0].slice();
        pending.take_pending();
        readers[0].consume(1);
        assert!(!pending.take_pending());
        readers[0].consume(1);
        assert!(pending.take_pending());
    }

    #[test]
    fn default_capacity_and_late_buffer_count_are_preserved() {
        let mut writer = Writer::<u32, LocalState<u32>, LocalBlockInbox>::new();
        writer.init(BlockId(0), PortIndex::new(0), local_inbox());
        let mut readers: Vec<_> = (0..2)
            .map(|i| {
                let mut reader = Reader::new();
                reader.init(BlockId(i + 1), PortIndex::new(0), local_inbox());
                reader.set_min_items(8);
                reader.set_min_buffers(2 + i);
                writer.connect(&mut reader);
                reader
            })
            .collect();
        let expected = config::config().buffer_size / u32::SIZE;
        for _ in 0..3 {
            assert_eq!(writer.slice().len(), expected);
            writer.produce(expected);
        }
        assert!(writer.slice().is_empty());
        assert_eq!(readers[0].slice().len(), expected);
        assert_eq!(readers[1].max_contiguous_items(), expected);
    }
}
