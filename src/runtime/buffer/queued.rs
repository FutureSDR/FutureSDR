use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
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
use crate::runtime::PortId;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortConfig;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::ThreadSafeConnect;
use crate::runtime::config;
use crate::runtime::dev::ItemTag;

#[derive(Debug)]
struct BufferEmpty<D: CpuSample> {
    buffer: Box<[D]>,
}

#[derive(Debug)]
struct BufferFull<D: CpuSample> {
    buffer: Box<[D]>,
    items: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct CurrentBuffer<D: CpuSample> {
    buffer: Box<[D]>,
    end_offset: usize,
    offset: usize,
    tags: Vec<ItemTag>,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct State<D: CpuSample> {
    writer_input: VecDeque<BufferEmpty<D>>,
    reader_input: VecDeque<BufferFull<D>>,
}

#[doc(hidden)]
pub trait SharedState<D: CpuSample>: Clone + Debug + 'static {
    fn new(state: State<D>) -> Self;
    fn with<R>(&self, f: impl FnOnce(&State<D>) -> R) -> R;
    fn with_mut<R>(&self, f: impl FnOnce(&mut State<D>) -> R) -> R;
}

#[doc(hidden)]
#[derive(Debug)]
pub struct LocalState<D: CpuSample>(Rc<RefCell<State<D>>>);

impl<D: CpuSample> Clone for LocalState<D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<D: CpuSample> SharedState<D> for LocalState<D> {
    fn new(state: State<D>) -> Self {
        Self(Rc::new(RefCell::new(state)))
    }

    fn with<R>(&self, f: impl FnOnce(&State<D>) -> R) -> R {
        f(&self.0.borrow())
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut State<D>) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct SendState<D: CpuSample>(Arc<Mutex<State<D>>>);

impl<D: CpuSample> Clone for SendState<D> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<D: CpuSample> SharedState<D> for SendState<D> {
    fn new(state: State<D>) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with<R>(&self, f: impl FnOnce(&State<D>) -> R) -> R {
        f(&self.0.lock().unwrap())
    }

    #[cfg(target_arch = "wasm32")]
    fn with<R>(&self, f: impl FnOnce(&State<D>) -> R) -> R {
        f(&self.0.lock())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_mut<R>(&self, f: impl FnOnce(&mut State<D>) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }

    #[cfg(target_arch = "wasm32")]
    fn with_mut<R>(&self, f: impl FnOnce(&mut State<D>) -> R) -> R {
        f(&mut self.0.lock())
    }
}

/// Queue-backed CPU writer.
#[derive(Debug)]
pub struct Writer<D, S, I = BlockInbox>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    core: PortCore<I>,
    state: ConnectionState<ConnectedWriter<D, S, I>>,
    current: Option<CurrentBuffer<D>>,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct ConnectedWriter<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    state: S,
    reserved_items: usize,
    reader: PortEndpoint<I>,
    _marker: PhantomData<D>,
}

/// Reader offer for a queue-backed cross-domain connection.
#[doc(hidden)]
pub struct ThreadSafeConnectToken<D, S>
where
    D: CpuSample,
    S: SharedState<D> + Send,
{
    reader: PortEndpoint<BlockInbox>,
    reader_min_items: Option<usize>,
    reader_min_buffer_size: Option<usize>,
    _state: PhantomData<fn() -> S>,
    _item: PhantomData<D>,
}

/// Reader installation returned by the queue-backed writer.
#[doc(hidden)]
pub struct ThreadSafeReturnToken<D, S>
where
    D: CpuSample,
    S: SharedState<D> + Send,
{
    connected: ConnectedReader<D, S, BlockInbox>,
    min_buffer_size: usize,
}

impl<D, S, I> Writer<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    /// Create a queue-backed CPU writer.
    pub fn new() -> Self {
        Self {
            core: PortCore::with_config(PortConfig::with_min_items(1)),
            state: ConnectionState::disconnected(),
            current: None,
            tags: Vec::new(),
        }
    }
}

impl<D, S, I> Default for Writer<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D, S, I> BufferWriter for Writer<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    type Inbox = I;
    type Reader = Reader<D, S, I>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: I) {
        self.core.init(block_id, port_id, inbox);
    }

    fn buffer_requirements(&self) -> BufferRequirements {
        let mut requirements = self.core.requirements();
        requirements.set_max_readers(1);
        requirements
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
        let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
            || dest.core.min_buffer_size_in_items().is_some();
        let reserved_items = dest.core.min_items().unwrap_or(0);

        let mut min_items = if buffer_size_configured {
            let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
            let min_reader = dest.core.min_buffer_size_in_items().unwrap_or(0);
            // `reserved_items` are look-ahead items kept before the readable
            // slice so readers with `set_min_items` can span chunk boundaries.
            // Configured buffer sizes describe usable items, so allocate the
            // requested usable size in addition to the reserved prefix.
            reserved_items + std::cmp::max(min_self, min_reader)
        } else {
            config::config().buffer_size / D::SIZE.get()
        };

        min_items = std::cmp::max(min_items, reserved_items + 1);

        let state = S::new(State {
            writer_input: VecDeque::new(),
            reader_input: VecDeque::new(),
        });
        state.with_mut(|state| {
            for _ in 0..2 {
                state.writer_input.push_back(BufferEmpty {
                    buffer: vec![D::default(); min_items].into_boxed_slice(),
                });
            }
        });

        self.core
            .set_min_buffer_size_in_items(min_items - reserved_items);
        dest.core
            .set_min_buffer_size_in_items(min_items - reserved_items);

        self.state.set_connected(ConnectedWriter {
            state: state.clone(),
            reserved_items,
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
            _marker: PhantomData,
        });
        dest.state.set_connected(ConnectedReader {
            state,
            reserved_items,
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
            _marker: PhantomData,
        });
    }

    async fn notify_finished(&mut self) {
        let connected = self.state.connected();
        let reserved_items = connected.reserved_items;
        if let Some(CurrentBuffer {
            buffer,
            offset,
            tags,
            ..
        }) = self.current.take()
            && offset > reserved_items
        {
            connected.state.with_mut(|state| {
                state.reader_input.push_back(BufferFull {
                    buffer,
                    items: offset - reserved_items,
                    tags,
                });
            });
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

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D, S> ThreadSafeConnect for Writer<D, S, BlockInbox>
where
    D: CpuSample,
    S: SharedState<D> + Send,
{
    type ReaderToken = ThreadSafeConnectToken<D, S>;
    type WriterToken = ThreadSafeReturnToken<D, S>;

    fn take_reader_token(reader: &mut Reader<D, S, BlockInbox>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox(), reader.core.port_id()),
            reader_min_items: reader.core.min_items(),
            reader_min_buffer_size: reader.core.min_buffer_size_in_items(),
            _state: PhantomData,
            _item: PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
            || token.reader_min_buffer_size.is_some();
        let reserved_items = token.reader_min_items.unwrap_or(0);

        let mut min_items = if buffer_size_configured {
            let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
            let min_reader = token.reader_min_buffer_size.unwrap_or(0);
            reserved_items + std::cmp::max(min_self, min_reader)
        } else {
            config::config().buffer_size / D::SIZE.get()
        };

        min_items = std::cmp::max(min_items, reserved_items + 1);
        let min_buffer_size = min_items - reserved_items;

        let state = S::new(State {
            writer_input: VecDeque::new(),
            reader_input: VecDeque::new(),
        });
        state.with_mut(|state| {
            for _ in 0..2 {
                state.writer_input.push_back(BufferEmpty {
                    buffer: vec![D::default(); min_items].into_boxed_slice(),
                });
            }
        });

        self.core.set_min_buffer_size_in_items(min_buffer_size);
        self.state.set_connected(ConnectedWriter {
            state: state.clone(),
            reserved_items,
            reader: token.reader,
            _marker: PhantomData,
        });

        ThreadSafeReturnToken {
            connected: ConnectedReader {
                state,
                reserved_items,
                writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
                _marker: PhantomData,
            },
            min_buffer_size,
        }
    }

    fn finish_reader(reader: &mut Reader<D, S, BlockInbox>, token: Self::WriterToken) {
        reader
            .core
            .set_min_buffer_size_in_items(token.min_buffer_size);
        reader.state.set_connected(token.connected);
    }
}

impl<D, S, I> CpuBufferWriter for Writer<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            let connected = self.state.connected();
            let next = connected
                .state
                .with_mut(|state| state.writer_input.pop_front());
            match next {
                Some(b) => {
                    let end_offset = b.buffer.len();
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: connected.reserved_items,
                        end_offset,
                        tags: Vec::new(),
                    });
                }
                None => return (&mut [], Tags::new(&mut self.tags, 0)),
            }
        }

        let c = self.current.as_mut().unwrap();
        (&mut c.buffer[c.offset..], Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let connected = self.state.connected();
        let reserved_items = connected.reserved_items;
        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        for t in self.tags.iter_mut() {
            t.index += c.offset;
        }
        c.tags.append(&mut self.tags);
        c.offset += n;

        if (c.end_offset - c.offset) < self.core.min_items().unwrap_or(1) {
            let c = self.current.take().unwrap();
            let has_writer_input = connected.state.with_mut(|state| {
                state.reader_input.push_back(BufferFull {
                    buffer: c.buffer,
                    items: c.offset - reserved_items,
                    tags: c.tags,
                });
                !state.writer_input.is_empty()
            });

            connected.reader.inbox().notify();
            if has_writer_input {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("set_min_items called after buffer is created. this has no effect");
        }
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!(
                "set_min_buffer_size_in_items called after buffer is created. this has no effect"
            );
        }
        self.core.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}

/// Queue-backed CPU reader.
#[derive(Debug)]
pub struct Reader<D, S, I = BlockInbox>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    core: PortCore<I>,
    state: ConnectionState<ConnectedReader<D, S, I>>,
    current: Option<CurrentBuffer<D>>,
    tags: Vec<ItemTag>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    state: S,
    reserved_items: usize,
    writer: PortEndpoint<I>,
    _marker: PhantomData<D>,
}

impl<D, S, I> Reader<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    /// Create a queue-backed CPU reader.
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            current: None,
            tags: Vec::new(),
            finished: false,
        }
    }
}

impl<D, S, I> Default for Reader<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D, S, I> BufferReader for Reader<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    type Inbox = I;
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: I) {
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
        let connected = self.state.connected();
        let _ = connected
            .writer
            .inbox()
            .stream_output_done(connected.writer.port_id())
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
                .is_none_or(|state| state.state.with(|state| state.reader_input.is_empty()))
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D, S, I> CpuBufferReader for Reader<D, S, I>
where
    D: CpuSample,
    S: SharedState<D>,
    I: BufferInbox,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        let connected = self.state.connected();
        let reserved_items = connected.reserved_items;

        if let Some(cur) = self.current.as_mut() {
            let left = cur.end_offset - cur.offset;
            debug_assert!(left > 0);
            if left <= reserved_items {
                let next = connected
                    .state
                    .with_mut(|state| state.reader_input.pop_front());
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = next
                {
                    let old_offset = cur.offset;
                    let old_end_offset = cur.end_offset;
                    let new_offset = reserved_items - left;

                    buffer[new_offset..reserved_items]
                        .clone_from_slice(&cur.buffer[old_offset..old_end_offset]);

                    cur.tags = cur
                        .tags
                        .drain(..)
                        .filter_map(|mut tag| {
                            if tag.index >= old_offset && tag.index < old_end_offset {
                                tag.index = new_offset + (tag.index - old_offset);
                                Some(tag)
                            } else {
                                None
                            }
                        })
                        .collect();
                    cur.tags.append(&mut tags);

                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    connected.state.with_mut(|state| {
                        state.writer_input.push_back(BufferEmpty { buffer: old })
                    });
                    connected.writer.inbox().notify();

                    cur.end_offset = reserved_items + items;
                    cur.offset = new_offset;
                }
            }
        } else {
            let next = connected
                .state
                .with_mut(|state| state.reader_input.pop_front());
            match next {
                Some(b) => {
                    let end_offset = b.items + reserved_items;
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: reserved_items,
                        end_offset,
                        tags: b.tags,
                    });
                }
                None => {
                    static V: Vec<ItemTag> = vec![];
                    return (&[], &V);
                }
            }
        }

        let c = self.current.as_ref().unwrap();
        self.tags.clear();
        self.tags.extend(c.tags.iter().filter_map(|tag| {
            if tag.index >= c.offset && tag.index < c.end_offset {
                let mut tag = tag.clone();
                tag.index -= c.offset;
                Some(tag)
            } else {
                None
            }
        }));
        (&c.buffer[c.offset..c.end_offset], &self.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let connected = self.state.connected();
        let reserved_items = connected.reserved_items;
        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        c.offset += n;
        c.tags.retain(|tag| tag.index >= c.offset);

        if c.offset == c.end_offset {
            let b = self.current.take().unwrap();
            let has_reader_input = connected.state.with_mut(|state| {
                state
                    .writer_input
                    .push_back(BufferEmpty { buffer: b.buffer });
                !state.reader_input.is_empty()
            });

            connected.writer.inbox().notify();
            if has_reader_input {
                self.core.inbox().notify();
            }
        } else {
            // This reader still has immediately readable data in its current
            // buffer. Wake the owning block again; otherwise a block that had
            // to stop early because an output buffer was full can go to sleep
            // even though unread input remains in this slab chunk.
            self.core.inbox().notify();

            if c.end_offset - c.offset <= reserved_items
                && connected.state.with(|state| !state.reader_input.is_empty())
            {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.core.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::runtime::block_inbox::LocalBlockInbox;
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::block_on;
    use crate::runtime::buffer::local;

    fn local_inbox() -> LocalBlockInbox {
        let (inbox, _rx) = LocalBlockInboxReader::pair();
        inbox
    }

    #[test]
    fn local_cpu_buffer_moves_items() -> Result<(), Error> {
        let mut writer = local::Writer::<u8>::default();
        let mut reader = local::Reader::<u8>::default();

        BufferWriter::init(&mut writer, BlockId(0), PortId::new("out"), local_inbox());
        BufferReader::init(&mut reader, BlockId(1), PortId::new("in"), local_inbox());

        CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 5);
        CpuBufferReader::set_min_items(&mut reader, 1);
        BufferWriter::connect(&mut writer, &mut reader);

        BufferWriter::validate(&writer)?;
        BufferReader::validate(&reader)?;

        let out = CpuBufferWriter::slice(&mut writer);
        out[..4].copy_from_slice(&[1, 2, 3, 4]);
        CpuBufferWriter::produce(&mut writer, 4);
        block_on(BufferWriter::notify_finished(&mut writer));

        let input = CpuBufferReader::slice(&mut reader);
        assert_eq!(input, &[1, 2, 3, 4]);

        CpuBufferReader::consume(&mut reader, 4);
        assert!(CpuBufferReader::slice(&mut reader).is_empty());

        Ok(())
    }

    #[test]
    fn local_cpu_buffer_flushes_partial_buffer_on_finish() -> Result<(), Error> {
        let mut writer = local::Writer::<u8>::default();
        let mut reader = local::Reader::<u8>::default();

        BufferWriter::init(&mut writer, BlockId(0), PortId::new("out"), local_inbox());
        BufferReader::init(&mut reader, BlockId(1), PortId::new("in"), local_inbox());

        CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 8);
        BufferWriter::connect(&mut writer, &mut reader);

        let out = CpuBufferWriter::slice(&mut writer);
        out[..3].copy_from_slice(&[9, 8, 7]);
        CpuBufferWriter::produce(&mut writer, 3);
        assert!(CpuBufferReader::slice(&mut reader).is_empty());

        block_on(BufferWriter::notify_finished(&mut writer));

        let input = CpuBufferReader::slice(&mut reader);
        assert_eq!(input, &[9, 8, 7]);

        Ok(())
    }
}
