use std::any::Any;
use std::fmt;
use std::mem::size_of;

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
use vmcircbuffer::Metadata;
use vmcircbuffer::lockfree as vm_lockfree;

struct TagMetadata {
    tags: Vec<ItemTag>,
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
    inbox: BlockInbox,
    block_id: BlockId,
    port_id: PortId,
    writer: Option<vm_lockfree::Writer<T, TagMetadata>>,
    readers: Vec<(PortId, BlockInbox)>,
    reader_notifiers: Vec<BlockNotifier>,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T, const MAX_READERS: usize> Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inbox: BlockInbox::default(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            writer: None,
            readers: Vec::new(),
            reader_notifiers: Vec::new(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
            min_items: None,
            min_buffer_size_in_items: None,
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
            .field("port_id", &self.port_id)
            .field("readers", &self.readers.len())
            .finish()
    }
}

impl<T, const MAX_READERS: usize> BufferWriter for Writer<T, MAX_READERS>
where
    T: CpuSample,
{
    type Reader = Reader<T, MAX_READERS>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.notifier = inbox.notifier();
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        if self.writer.is_none() {
            let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
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

            let capacity = buffer_size / size_of::<T>();
            self.min_buffer_size_in_items = Some(capacity);
            dest.min_buffer_size_in_items = Some(capacity);
            self.writer = Some(
                vm_lockfree::Circular::with_capacity::<T, TagMetadata>(capacity, MAX_READERS)
                    .expect("failed to allocate perf::lockfree buffer"),
            );
        } else {
            if self.min_buffer_size_in_items.unwrap_or(0)
                < dest.min_buffer_size_in_items.unwrap_or(0)
            {
                warn!(
                    "lockfree buffer is already created, size constraints of reader are not considered."
                );
            }
            if self.min_buffer_size_in_items.unwrap_or(0) - self.min_items.unwrap_or(0) + 1
                < dest.min_items.unwrap_or(1)
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

        self.readers.push((dest.port_id.clone(), dest.inbox.clone()));
        self.reader_notifiers.push(dest.notifier.clone());

        dest.reader = Some(reader);
        dest.writer_output_id = self.port_id.clone();
        dest.writer_inbox = self.inbox.clone();
        dest.writer_notifier = self.notifier.clone();
    }

    async fn notify_finished(&mut self) {
        for (input_id, inbox) in &mut self.readers {
            let _ = inbox
                .send(BlockMessage::StreamInputDone {
                    input_id: input_id.clone(),
                })
                .await;
        }
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
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

    fn set_min_items(&mut self, n: usize) {
        if self.writer.is_some() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_items = Some(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.writer.is_some() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_buffer_size_in_items = Some(n);
    }

    fn max_items(&self) -> usize {
        self.min_buffer_size_in_items.unwrap_or(usize::MAX)
    }
}

pub struct Reader<T, const MAX_READERS: usize>
where
    T: CpuSample,
{
    reader: Option<vm_lockfree::Reader<T, TagMetadata>>,
    finished: bool,
    writer_inbox: BlockInbox,
    writer_output_id: PortId,
    writer_notifier: BlockNotifier,
    block_id: BlockId,
    port_id: PortId,
    inbox: BlockInbox,
    notifier: BlockNotifier,
    tags: Vec<ItemTag>,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<T, const MAX_READERS: usize> Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            reader: None,
            finished: false,
            writer_inbox: BlockInbox::default(),
            writer_output_id: PortId::default(),
            writer_notifier: BlockNotifier::new(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inbox: BlockInbox::default(),
            notifier: BlockNotifier::new(),
            tags: Vec::new(),
            min_items: None,
            min_buffer_size_in_items: None,
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
            .field("port_id", &self.port_id)
            .field("finished", &self.finished)
            .finish()
    }
}

#[async_trait]
impl<T, const MAX_READERS: usize> BufferReader for Reader<T, MAX_READERS>
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
        if self.reader.is_some() {
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

impl<T, const MAX_READERS: usize> CpuBufferReader for Reader<T, MAX_READERS>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        self.reader.as_mut().expect("reader not connected").slice()
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
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
        self.reader.as_mut().expect("reader not connected").consume(n);
        if n > 0 {
            self.writer_notifier.notify();
        }
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
        self.min_buffer_size_in_items.unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::runtime::Tag;

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
