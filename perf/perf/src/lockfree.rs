use std::fmt;

use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortIndex;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::buffer::dev::BlockInbox;
use futuresdr::runtime::buffer::dev::BufferRequirements;
use futuresdr::runtime::buffer::dev::PortCore;
use futuresdr::runtime::buffer::dev::PortEndpoint;
use futuresdr::runtime::dev::BlockNotifier;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::tracing::warn;
use vmcircbuffer::Metadata;
use vmcircbuffer::lockfree as vm_lockfree;

struct TagMetadata {
    tags: Vec<ItemTag>,
}

pub struct ThreadSafeConnectToken<T, const MAX_READERS: usize>
where
    T: CpuSample,
{
    reader: PortEndpoint,
    reader_notifier: BlockNotifier,
    reader_min_items: Option<usize>,
    reader_min_buffer_size_in_items: Option<usize>,
    _item: std::marker::PhantomData<fn() -> T>,
}

pub struct ThreadSafeReturnToken<T, const MAX_READERS: usize>
where
    T: CpuSample,
{
    reader: vm_lockfree::Reader<T, TagMetadata>,
    writer: PortEndpoint,
    writer_notifier: BlockNotifier,
    min_buffer_size_in_items: usize,
}

impl Metadata for TagMetadata {
    type Item = ItemTag;

    fn new() -> Self {
        Self { tags: Vec::new() }
    }

    fn add_from_slice(&mut self, offset: usize, tags: &[Self::Item]) {
        for tag in tags {
            let mut tag = tag.clone();
            tag.index += offset;
            self.tags.push(tag);
        }
    }

    fn get_into(&self, out: &mut Vec<Self::Item>) {
        out.clear();
        out.extend(self.tags.iter().cloned());
    }

    fn consume(&mut self, items: usize) {
        self.tags.retain(|tag| tag.index >= items);
        for tag in &mut self.tags {
            tag.index -= items;
        }
    }
}

pub struct Writer<T, const MAX_READERS: usize>
where
    T: CpuSample,
{
    core: PortCore,
    writer: Option<vm_lockfree::Writer<T, TagMetadata>>,
    readers: Vec<PortEndpoint>,
    reader_notifiers: Vec<BlockNotifier>,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
}

impl<T, const MAX_READERS: usize> Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            core: PortCore::new_unbound(),
            writer: None,
            readers: Vec::new(),
            reader_notifiers: Vec::new(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
        }
    }
}

impl<T, const MAX_READERS: usize> Default for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const MAX_READERS: usize> fmt::Debug for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("perf::lockfree::Writer")
            .field("port_id", &self.core.port_id_if_bound())
            .field("readers", &self.reader_notifiers.len())
            .finish()
    }
}

impl<T, const MAX_READERS: usize> BufferWriter for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    type Inbox = BlockInbox;
    type Reader = Reader<T, MAX_READERS>;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.core.requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.core.raise_requirements(requirements);
    }

    fn max_readers(&self) -> usize {
        MAX_READERS
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, inbox: BlockInbox) {
        self.notifier = inbox.notifier();
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer.is_some() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        if self.writer.is_none() {
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
            self.writer = Some(
                vm_lockfree::Circular::with_capacity::<T, TagMetadata>(capacity, MAX_READERS)
                    .expect("failed to allocate perf::lockfree buffer"),
            );
        } else {
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                < dest.core.min_buffer_size_in_items().unwrap_or(0)
            {
                warn!(
                    "lockfree buffer is already created, size constraints of reader are not considered."
                );
            }
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                - self.core.min_items().unwrap_or(0)
                + 1
                < dest.core.min_items().unwrap_or(1)
            {
                warn!(
                    "lockfree buffer is already created, size constraints of reader are not considered."
                );
            }
        }

        let reader = self
            .writer
            .as_ref()
            .unwrap()
            .add_reader()
            .expect("perf::lockfree reader limit exceeded");

        if let Some(reader) = dest.core.endpoint_if_bound() {
            self.readers.push(reader);
        }
        self.reader_notifiers.push(dest.notifier.clone());

        dest.reader = Some(reader);
        dest.writer = self.core.endpoint_if_bound();
        dest.writer_notifier = self.notifier.clone();
    }

    async fn notify_finished(&mut self) {
        for reader in &self.readers {
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

impl<T, const MAX_READERS: usize> ThreadSafeConnect for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    type ReaderToken = ThreadSafeConnectToken<T, MAX_READERS>;
    type WriterToken = ThreadSafeReturnToken<T, MAX_READERS>;

    fn take_reader_token(reader: &mut Reader<T, MAX_READERS>) -> Self::ReaderToken {
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
        let min_buffer_size_in_items = if self.writer.is_some() {
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                < token.reader_min_buffer_size_in_items.unwrap_or(0)
            {
                warn!(
                    "lockfree buffer is already created, size constraints of reader are not considered."
                );
            }
            if self.core.min_buffer_size_in_items().unwrap_or(0)
                - self.core.min_items().unwrap_or(0)
                + 1
                < token.reader_min_items.unwrap_or(1)
            {
                warn!(
                    "lockfree buffer is already created, size constraints of reader are not considered."
                );
            }
            self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
        } else {
            let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
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

            let capacity = buffer_size / T::SIZE.get();
            self.writer = Some(
                vm_lockfree::Circular::with_capacity::<T, TagMetadata>(capacity, MAX_READERS)
                    .expect("failed to allocate perf::lockfree buffer"),
            );
            self.core.set_min_buffer_size_in_items(capacity);
            capacity
        };

        let reader = self
            .writer
            .as_ref()
            .expect("writer was initialized above")
            .add_reader()
            .expect("perf::lockfree reader limit exceeded");
        self.readers.push(token.reader);
        self.reader_notifiers.push(token.reader_notifier);

        ThreadSafeReturnToken {
            reader,
            writer: self
                .core
                .endpoint_if_bound()
                .expect("writer port not bound to a flowgraph"),
            writer_notifier: self.notifier.clone(),
            min_buffer_size_in_items,
        }
    }

    fn finish_reader(reader: &mut Reader<T, MAX_READERS>, token: Self::WriterToken) {
        reader
            .core
            .set_min_buffer_size_in_items(token.min_buffer_size_in_items);
        reader.reader = Some(token.reader);
        reader.writer = Some(token.writer);
        reader.writer_notifier = token.writer_notifier;
    }
}

impl<T, const MAX_READERS: usize> CpuBufferWriter for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        self.writer.as_mut().expect("writer not connected").slice()
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        let tags = &mut self.tags as *mut Vec<ItemTag>;
        let slice = self.writer.as_mut().expect("writer not connected").slice();
        unsafe { (slice, Tags::new(&mut *tags, 0)) }
    }

    fn produce(&mut self, n: usize) {
        self.writer
            .as_mut()
            .expect("writer not connected")
            .produce(n, &self.tags);
        self.tags.clear();

        if n > 0 {
            for notifier in &self.reader_notifiers {
                notifier.notify();
            }
        }
    }
}

pub struct Reader<T, const MAX_READERS: usize>
where
    T: CpuSample,
{
    reader: Option<vm_lockfree::Reader<T, TagMetadata>>,
    finished: bool,
    writer: Option<PortEndpoint>,
    writer_notifier: BlockNotifier,
    core: PortCore,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
}

impl<T, const MAX_READERS: usize> Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            reader: None,
            finished: false,
            writer: None,
            writer_notifier: BlockNotifier::new(),
            core: PortCore::new_unbound(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
        }
    }
}

impl<T, const MAX_READERS: usize> Default for Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const MAX_READERS: usize> fmt::Debug for Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("perf::lockfree::Reader")
            .field("port_id", &self.core.port_id_if_bound())
            .field("finished", &self.finished)
            .finish()
    }
}

impl<T, const MAX_READERS: usize> BufferReader for Reader<T, MAX_READERS>
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
        if self.reader.is_some() {
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

impl<T, const MAX_READERS: usize> CpuBufferReader for Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        self.reader.as_mut().expect("reader not connected").slice()
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        match self
            .reader
            .as_mut()
            .expect("reader not connected")
            .slice_with_meta_into(&mut self.tags)
        {
            Some(slice) => (slice, &self.tags),
            None => {
                debug_assert!(self.tags.is_empty());
                (&[], &self.tags)
            }
        }
    }

    fn consume(&mut self, n: usize) {
        self.reader
            .as_mut()
            .expect("reader not connected")
            .consume(n);
        if n > 0 {
            self.writer_notifier.notify();
        }
    }

    fn max_contiguous_items(&self) -> usize {
        self.core
            .min_buffer_size_in_items()
            .expect("lock-free buffer capacity missing after validation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::runtime::dev::Tag;

    #[test]
    fn basic_transfer() {
        let mut writer = Writer::<u32, 1>::default();
        let mut reader = Reader::<u32, 1>::default();
        writer.connect(&mut reader);

        let out = writer.slice();
        out[..4].copy_from_slice(&[1, 2, 3, 4]);
        writer.produce(4);

        let input = reader.slice();
        assert_eq!(&input[..4], &[1, 2, 3, 4]);
        reader.consume(4);
        assert!(reader.slice().is_empty());
    }

    #[test]
    fn tags_are_propagated() {
        let mut writer = Writer::<u32, 1>::default();
        let mut reader = Reader::<u32, 1>::default();
        writer.connect(&mut reader);

        let (out, mut tags) = writer.slice_with_tags();
        out[..2].copy_from_slice(&[7, 8]);
        tags.add_tag(1, Tag::NamedUsize("mark".to_string(), 23));
        writer.produce(2);

        let (input, in_tags) = reader.slice_with_tags();
        assert_eq!(&input[..2], &[7, 8]);
        assert_eq!(in_tags.len(), 1);
        assert_eq!(in_tags[0].index, 1);
        reader.consume(2);
    }
}
