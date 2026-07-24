use std::any::Any;
use std::fmt;

use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortId;
use futuresdr::runtime::buffer::BufferInbox;
use futuresdr::runtime::buffer::BufferNotifier;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::ConnectionState;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::LocalBlockInbox;
use futuresdr::runtime::buffer::PortCore;
use futuresdr::runtime::buffer::PortEndpoint;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::tracing::warn;
use vmcircbuffer::generic;

struct LocalNotifier<N: BufferNotifier> {
    notifier: N,
}

impl<N: BufferNotifier> generic::Notifier for LocalNotifier<N> {
    fn arm(&mut self) {}

    fn notify(&mut self) {
        self.notifier.notify();
    }
}

struct NoMetadata;

impl generic::Metadata for NoMetadata {
    type Item = ItemTag;

    fn new() -> Self {
        Self
    }

    fn add_from_slice(&mut self, _offset: usize, _tags: &[Self::Item]) {}

    fn get_into(&self, out: &mut Vec<Self::Item>) {
        out.clear();
    }

    fn consume(&mut self, _items: usize) {}
}

/// Same-thread circular CPU writer with one producer and multiple readers.
///
/// This buffer is intended for local domains. It uses the circular buffer
/// backend but deliberately drops all tags.
pub struct Writer<T>
where
    T: CpuSample,
{
    core: PortCore<LocalBlockInbox>,
    state: ConnectionState<ConnectedWriter<T>>,
    tags: Vec<ItemTag>,
}

struct ConnectedWriter<T>
where
    T: CpuSample,
{
    writer: generic::Writer<
        T,
        LocalNotifier<<LocalBlockInbox as futuresdr::runtime::buffer::BufferInbox>::Notifier>,
        NoMetadata,
    >,
    readers: Vec<PortEndpoint<LocalBlockInbox>>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            tags: Vec::new(),
        }
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
        f.debug_struct("perf::local_mpsc::Writer")
            .field("port_id", &self.core.port_id_if_bound())
            .field("readers", &self.state.as_ref().map(|s| s.readers.len()))
            .finish()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Inbox = LocalBlockInbox;
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: LocalBlockInbox) {
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
        let mut connected = if let Some(connected) = self.state.take_connected() {
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                < dest.core.min_buffer_size_in_items().unwrap_or(0)
            {
                warn!(
                    "local_mpsc buffer is already created, size constraints of reader are not considered."
                );
            }
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                - self.core.min_items().unwrap_or(0)
                + 1
                < dest.core.min_items().unwrap_or(1)
            {
                warn!(
                    "local_mpsc buffer is already created, size constraints of reader are not considered."
                );
            }
            connected
        } else {
            let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
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

            let capacity = buffer_size / T::SIZE.get();
            self.core.set_min_buffer_size_in_items(capacity);
            dest.core.set_min_buffer_size_in_items(capacity);

            ConnectedWriter {
                writer: generic::Circular::with_capacity(capacity)
                    .expect("failed to allocate perf::local_mpsc buffer"),
                readers: Vec::new(),
            }
        };

        let writer_notifier = LocalNotifier {
            notifier: self.core.notifier(),
        };
        let reader_notifier = LocalNotifier {
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
        for reader in &self.state.connected().readers {
            let _ = reader.inbox().stream_input_done(reader.port_id()).await;
        }
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        self.state.connected_mut().writer.slice(false)
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        self.tags.clear();
        let tags = &mut self.tags as *mut Vec<ItemTag>;
        let slice = self.state.connected_mut().writer.slice(false);
        unsafe { (slice, Tags::new(&mut *tags, 0)) }
    }

    fn produce(&mut self, items: usize) {
        self.tags.clear();
        self.state.connected_mut().writer.produce(items, &[]);
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

/// Same-thread circular CPU reader for [`Writer`].
pub struct Reader<T>
where
    T: CpuSample,
{
    state: ConnectionState<ConnectedReader<T>>,
    finished: bool,
    core: PortCore<LocalBlockInbox>,
    tags: Vec<ItemTag>,
}

struct ConnectedReader<T>
where
    T: CpuSample,
{
    reader: generic::Reader<
        T,
        LocalNotifier<<LocalBlockInbox as futuresdr::runtime::buffer::BufferInbox>::Notifier>,
        NoMetadata,
    >,
    writer: PortEndpoint<LocalBlockInbox>,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            state: ConnectionState::disconnected(),
            finished: false,
            core: PortCore::new_disconnected(),
            tags: Vec::new(),
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
        f.debug_struct("perf::local_mpsc::Reader")
            .field(
                "writer_output_id",
                &self.state.as_ref().map(|state| state.writer.port_id()),
            )
            .field("finished", &self.finished)
            .finish()
    }
}

impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
{
    type Inbox = LocalBlockInbox;

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: LocalBlockInbox) {
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

impl<T> CpuBufferReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        self.state
            .connected_mut()
            .reader
            .slice(false)
            .unwrap_or(&[])
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        self.tags.clear();
        let slice = self
            .state
            .connected_mut()
            .reader
            .slice(false)
            .unwrap_or(&[]);
        (slice, &self.tags)
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

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::runtime::dev::LocalBlockInbox;
    use futuresdr::runtime::dev::Tag;

    fn init<T: CpuSample>(w: &mut Writer<T>, readers: &mut [&mut Reader<T>]) {
        w.init(BlockId(0), PortId::from("out"), LocalBlockInbox::default());
        for (i, reader) in readers.iter_mut().enumerate() {
            reader.init(
                BlockId(i + 1),
                PortId::from(format!("in{i}")),
                LocalBlockInbox::default(),
            );
        }
    }

    #[test]
    fn fanout() {
        let mut w = Writer::<u32>::default();
        let mut r0 = Reader::<u32>::default();
        let mut r1 = Reader::<u32>::default();
        init(&mut w, &mut [&mut r0, &mut r1]);
        w.connect(&mut r0);
        w.connect(&mut r1);

        let out = w.slice();
        out[..4].copy_from_slice(&[1, 2, 3, 4]);
        w.produce(4);

        assert_eq!(&r0.slice()[..4], &[1, 2, 3, 4]);
        assert_eq!(&r1.slice()[..4], &[1, 2, 3, 4]);
        r0.consume(4);
        assert_eq!(&r1.slice()[..4], &[1, 2, 3, 4]);
        r1.consume(4);
    }

    #[test]
    fn tags_are_ignored() {
        let mut w = Writer::<u32>::default();
        let mut r = Reader::<u32>::default();
        init(&mut w, &mut [&mut r]);
        w.connect(&mut r);

        let (out, mut tags) = w.slice_with_tags();
        out[..2].copy_from_slice(&[7, 8]);
        tags.add_tag(1, Tag::NamedUsize("ignored".to_string(), 23));
        w.produce(2);

        let (input, in_tags) = r.slice_with_tags();
        assert_eq!(&input[..2], &[7, 8]);
        assert!(in_tags.is_empty());
    }
}
