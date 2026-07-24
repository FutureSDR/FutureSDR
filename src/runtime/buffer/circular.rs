use std::fmt;
use vmcircbuffer::generic;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferInbox;
use crate::runtime::buffer::BufferNotifier;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::ThreadSafeConnect;
use crate::runtime::config::config;
use crate::runtime::dev::ItemTag;

struct MyNotifier<N: BufferNotifier> {
    notifier: N,
}

impl<N: BufferNotifier> generic::Notifier for MyNotifier<N> {
    // we never arm the notifier
    fn arm(&mut self) {}

    // we notify blocks for every change to the buffer
    fn notify(&mut self) {
        self.notifier.notify();
    }
}

struct MyMetadata {
    tags: Vec<ItemTag>,
}

impl generic::Metadata for MyMetadata {
    type Item = ItemTag;

    fn new() -> Self {
        MyMetadata { tags: Vec::new() }
    }
    fn add_from_slice(&mut self, offset: usize, tags: &[Self::Item]) {
        for t in tags {
            let mut t = t.clone();
            t.index += offset;
            self.tags.push(t);
        }
    }
    fn get_into(&self, out: &mut Vec<Self::Item>) {
        out.clear();
        out.extend(self.tags.iter().cloned());
    }
    fn consume(&mut self, items: usize) {
        self.tags.retain(|x| x.index >= items);
        for t in self.tags.iter_mut() {
            t.index -= items;
        }
    }
}

/// Circular writer
pub struct Writer<D, I = BlockInbox>
where
    D: CpuSample,
    I: BufferInbox,
{
    core: PortCore<I>,
    state: ConnectionState<ConnectedWriter<D, I>>,
    finished: bool,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    writer: generic::Writer<D, MyNotifier<I::Notifier>, MyMetadata>,
    readers: Vec<PortEndpoint<I>>,
}

/// Reader offer for a circular-buffer cross-domain connection.
#[doc(hidden)]
pub struct ThreadSafeConnectToken<D>
where
    D: CpuSample,
{
    reader: PortEndpoint<BlockInbox>,
    reader_min_items: Option<usize>,
    reader_min_buffer_size: Option<usize>,
    _item: std::marker::PhantomData<D>,
}

/// Reader installation returned by the circular writer.
#[doc(hidden)]
pub struct ThreadSafeReturnToken<D>
where
    D: CpuSample,
{
    connected: ConnectedReader<D, BlockInbox>,
    min_buffer_size: usize,
}

impl<D, I> Writer<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            state: ConnectionState::disconnected(),
            finished: false,
            tags: vec![],
        }
    }
}

impl<D, I> Default for Writer<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D, I> BufferWriter for Writer<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    type Inbox = I;
    type Reader = Reader<D, I>;

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: I) {
        self.core.init(block_id, port_id, inbox);
    }

    fn max_readers(&self) -> usize {
        usize::MAX
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
        let mut connected = if let Some(connected) = self.state.take_connected() {
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                < dest.core.min_buffer_size_in_items().unwrap_or(0)
            {
                warn!(
                    "circular buffer is already created, size constraints of reader are not considered."
                );
                warn!(
                    "buffer size is {:?}, reader requirement {:?}",
                    self.core.min_buffer_size_in_items(),
                    dest.core.min_buffer_size_in_items()
                );
            }
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                - self.core.min_items().unwrap_or(0)
                + 1
                < dest.core.min_items().unwrap_or(1)
            {
                warn!(
                    "circular buffer is already created, size constraints of reader are not considered."
                );
                warn!(
                    "buffer size is {:?}, writer min items {:?}",
                    self.core.min_buffer_size_in_items(),
                    self.core.min_items()
                );
            }
            connected
        } else {
            let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
            let mut buffer_size = page_size;

            // Items required for work() to proceed
            let min_self = self.core.min_items().unwrap_or(1);
            let min_reader = dest.core.min_items().unwrap_or(1);
            let mut min_bytes = (min_self + min_reader - 1) * D::SIZE.get();

            let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
                || dest.core.min_buffer_size_in_items().is_some();

            min_bytes = if buffer_size_configured {
                let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
                let min_reader = dest.core.min_buffer_size_in_items().unwrap_or(0);
                std::cmp::max(
                    min_bytes,
                    std::cmp::max(min_self, min_reader) * D::SIZE.get(),
                )
            } else {
                std::cmp::max(min_bytes, config().buffer_size)
            };

            while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(D::SIZE.get()) {
                buffer_size += page_size;
            }

            self.core
                .set_min_buffer_size_in_items(buffer_size / D::SIZE.get());
            dest.core
                .set_min_buffer_size_in_items(buffer_size / D::SIZE.get());

            ConnectedWriter {
                writer: generic::Circular::with_capacity(buffer_size / D::SIZE.get()).unwrap(),
                readers: vec![],
            }
        };

        let writer_notifier = MyNotifier {
            notifier: self.core.notifier(),
        };

        let reader_notifier = MyNotifier {
            notifier: dest.core.notifier(),
        };

        let reader = connected
            .writer
            .add_reader(reader_notifier, writer_notifier);

        connected
            .readers
            .push(PortEndpoint::new(dest.core.inbox(), dest.core.port_id()));
        self.state.set_connected(connected);

        dest.state.set_connected(ConnectedReader {
            reader,
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
        });
    }
    async fn notify_finished(&mut self) {
        for i in &self.state.connected().readers {
            let _ = i.inbox().stream_input_done(i.port_id()).await;
        }
    }
    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }
    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D> ThreadSafeConnect for Writer<D, BlockInbox>
where
    D: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<D>;
    type WriterToken = ThreadSafeReturnToken<D>;

    fn take_reader_token(reader: &mut Reader<D, BlockInbox>) -> Self::ReaderToken {
        ThreadSafeConnectToken {
            reader: PortEndpoint::new(reader.core.inbox(), reader.core.port_id()),
            reader_min_items: reader.core.min_items(),
            reader_min_buffer_size: reader.core.min_buffer_size_in_items(),
            _item: std::marker::PhantomData,
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        let min_buffer_size = if self.state.is_connected() {
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                < token.reader_min_buffer_size.unwrap_or(0)
            {
                warn!(
                    "circular buffer is already created, size constraints of reader are not considered."
                );
            }
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                - self.core.min_items().unwrap_or(0)
                + 1
                < token.reader_min_items.unwrap_or(1)
            {
                warn!(
                    "circular buffer is already created, size constraints of reader are not considered."
                );
            }
            self.core.min_buffer_size_in_items().unwrap_or(0)
        } else {
            let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
            let mut buffer_size = page_size;
            let min_self = self.core.min_items().unwrap_or(1);
            let min_reader = token.reader_min_items.unwrap_or(1);
            let mut min_bytes = (min_self + min_reader - 1) * D::SIZE.get();
            let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
                || token.reader_min_buffer_size.is_some();

            min_bytes = if buffer_size_configured {
                let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
                let min_reader = token.reader_min_buffer_size.unwrap_or(0);
                std::cmp::max(
                    min_bytes,
                    std::cmp::max(min_self, min_reader) * D::SIZE.get(),
                )
            } else {
                std::cmp::max(min_bytes, config().buffer_size)
            };

            while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(D::SIZE.get()) {
                buffer_size += page_size;
            }

            self.core
                .set_min_buffer_size_in_items(buffer_size / D::SIZE.get());
            self.state.set_connected(ConnectedWriter {
                writer: generic::Circular::with_capacity(buffer_size / D::SIZE.get()).unwrap(),
                readers: vec![],
            });
            buffer_size / D::SIZE.get()
        };

        let writer_notifier = MyNotifier {
            notifier: self.core.notifier(),
        };
        let reader_notifier = MyNotifier {
            notifier: BlockInbox::notifier(&token.reader.inbox()),
        };
        let connected = self.state.connected_mut();
        let reader = connected
            .writer
            .add_reader(reader_notifier, writer_notifier);
        connected.readers.push(token.reader);

        ThreadSafeReturnToken {
            connected: ConnectedReader {
                reader,
                writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
            },
            min_buffer_size,
        }
    }

    fn finish_reader(reader: &mut Reader<D, BlockInbox>, token: Self::WriterToken) {
        reader
            .core
            .set_min_buffer_size_in_items(token.min_buffer_size);
        reader.state.set_connected(token.connected);
    }
}

impl<D, I> CpuBufferWriter for Writer<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    type Item = D;

    fn slice(&mut self) -> &mut [Self::Item] {
        self.state.connected_mut().writer.slice(false)
    }

    fn produce(&mut self, items: usize) {
        self.state.connected_mut().writer.produce(items, &self.tags);
        self.tags.clear();
    }
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        let s = self.state.connected_mut().writer.slice(false);
        (s, Tags::new(&mut self.tags, 0))
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

impl<D, I> fmt::Debug for Writer<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("output_id", &self.core.port_id_if_bound())
            .field("finished", &self.finished)
            .finish()
    }
}

/// Circular Reader
pub struct Reader<D, I = BlockInbox>
where
    D: CpuSample,
    I: BufferInbox,
{
    state: ConnectionState<ConnectedReader<D, I>>,
    finished: bool,
    core: PortCore<I>,
    tags: Vec<ItemTag>,
}

struct ConnectedReader<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    reader: generic::Reader<D, MyNotifier<I::Notifier>, MyMetadata>,
    writer: PortEndpoint<I>,
}

impl<D, I> Default for Reader<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    fn default() -> Self {
        Self {
            state: ConnectionState::disconnected(),
            finished: false,
            core: PortCore::new_unbound(),
            tags: vec![],
        }
    }
}

impl<D, I> BufferReader for Reader<D, I>
where
    D: CpuSample,
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
    }
    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }
    fn port_id(&self) -> PortIndex {
        self.core.port_id()
    }
}

impl<D, I> CpuBufferReader for Reader<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    type Item = D;

    fn slice(&mut self) -> &[Self::Item] {
        self.state
            .connected_mut()
            .reader
            .slice(false)
            .unwrap_or(&[])
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        match self
            .state
            .connected_mut()
            .reader
            .slice_with_metadata_into(false, &mut self.tags)
        {
            Some(s) => (s, &self.tags),
            _ => {
                debug_assert!(self.tags.is_empty());
                (&[], &self.tags)
            }
        }
    }
    fn consume(&mut self, amount: usize) {
        self.state.connected_mut().reader.consume(amount);
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

impl<D, I> fmt::Debug for Reader<D, I>
where
    D: CpuSample,
    I: BufferInbox,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Reader")
            .field(
                "writer_output_id",
                &self.state.as_ref().map(|state| state.writer.port_id()),
            )
            .field("finished", &self.finished)
            .finish()
    }
}
