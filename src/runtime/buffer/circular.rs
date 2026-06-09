use std::any::Any;
use std::fmt;
use std::mem::size_of;
use vmcircbuffer::generic;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferInbox;
use crate::runtime::buffer::BufferMode;
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
use crate::runtime::buffer::ThreadSafeMode;
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
pub struct Writer<D, M = ThreadSafeMode>
where
    D: CpuSample,
    M: BufferMode,
{
    core: PortCore<M>,
    state: ConnectionState<ConnectedWriter<D, M>>,
    finished: bool,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    writer: generic::Writer<D, MyNotifier<M::Notifier>, MyMetadata>,
    readers: Vec<PortEndpoint<M>>,
}

impl<D, M> Writer<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            finished: false,
            tags: vec![],
        }
    }
}

impl<D, M> Default for Writer<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D, M> BufferWriter for Writer<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    type Mode = M;
    type Reader = Reader<D, M>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: M::Inbox) {
        self.core.init(block_id, port_id, inbox);
    }
    fn buffer_requirements(&self) -> BufferRequirements {
        let mut requirements = self.core.requirements();
        requirements.set_max_readers(usize::MAX);
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
            let mut min_bytes = (min_self + min_reader - 1) * size_of::<D>();

            let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
                || dest.core.min_buffer_size_in_items().is_some();

            min_bytes = if buffer_size_configured {
                let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
                let min_reader = dest.core.min_buffer_size_in_items().unwrap_or(0);
                std::cmp::max(
                    min_bytes,
                    std::cmp::max(min_self, min_reader) * size_of::<D>(),
                )
            } else {
                std::cmp::max(min_bytes, futuresdr::runtime::config::config().buffer_size)
            };

            while (buffer_size < min_bytes) || !buffer_size.is_multiple_of(size_of::<D>()) {
                buffer_size += page_size;
            }

            self.core
                .set_min_buffer_size_in_items(buffer_size / size_of::<D>());
            dest.core
                .set_min_buffer_size_in_items(buffer_size / size_of::<D>());

            ConnectedWriter {
                writer: generic::Circular::with_capacity(buffer_size / size_of::<D>()).unwrap(),
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
    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D, M> CpuBufferWriter for Writer<D, M>
where
    D: CpuSample,
    M: BufferMode,
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

impl<D, M> fmt::Debug for Writer<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("output_id", &self.core.port_id_if_bound())
            .field("finished", &self.finished)
            .finish()
    }
}

/// Circular Reader
pub struct Reader<D, M = ThreadSafeMode>
where
    D: CpuSample,
    M: BufferMode,
{
    state: ConnectionState<ConnectedReader<D, M>>,
    finished: bool,
    core: PortCore<M>,
    tags: Vec<ItemTag>,
}

struct ConnectedReader<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    reader: generic::Reader<D, MyNotifier<M::Notifier>, MyMetadata>,
    writer: PortEndpoint<M>,
}

impl<D, M> Default for Reader<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    fn default() -> Self {
        Self {
            state: ConnectionState::disconnected(),
            finished: false,
            core: PortCore::new_disconnected(),
            tags: vec![],
        }
    }
}

impl<D, M> BufferReader for Reader<D, M>
where
    D: CpuSample,
    M: BufferMode,
{
    type Mode = M;
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: M::Inbox) {
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
        let _ = self
            .state
            .connected()
            .writer
            .inbox()
            .stream_output_done(self.state.connected().writer.port_id())
            .await;
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
    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D, M> CpuBufferReader for Reader<D, M>
where
    D: CpuSample,
    M: BufferMode,
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

impl<D, M> fmt::Debug for Reader<D, M>
where
    D: CpuSample,
    M: BufferMode,
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
