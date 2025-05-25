use futures::prelude::*;
use std::any::Any;
use std::fmt;
use std::mem::size_of;
use vmcircbuffer::generic;

use crate::channel::mpsc::channel;
use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

struct MyNotifier {
    sender: Sender<BlockMessage>,
}

impl generic::Notifier for MyNotifier {
    fn arm(&mut self) {}

    fn notify(&mut self) {
        let _ = self.sender.try_send(BlockMessage::Notify);
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
    fn add(&mut self, offset: usize, mut tags: Vec<Self::Item>) {
        for t in tags.iter_mut() {
            t.index += offset;
        }
        self.tags.append(&mut tags);
    }
    fn get(&self) -> Vec<Self::Item> {
        self.tags.clone()
    }
    fn consume(&mut self, items: usize) {
        self.tags.retain(|x| x.index >= items);
        for t in self.tags.iter_mut() {
            t.index -= items;
        }
    }
}

/// Circular writer
pub struct Writer<D>
where
    D: CpuSample,
{
    inbox: Sender<BlockMessage>,
    block_id: BlockId,
    port_id: PortId,
    writer: Option<generic::Writer<D, MyNotifier, MyMetadata>>,
    readers: Vec<(PortId, Sender<BlockMessage>)>,
    finished: bool,
    tags: Vec<ItemTag>,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            inbox: rx,
            block_id: BlockId::default(),
            port_id: PortId::default(),
            writer: None,
            readers: vec![],
            finished: false,
            tags: vec![],
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }
}

impl<D> Default for Writer<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D> BufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
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

            // Items required for work() to proceed
            let min_self = self.min_items.unwrap_or(1);
            let min_reader = dest.min_items.unwrap_or(1);
            let mut min_bytes = (min_self + min_reader - 1) * size_of::<D>();

            let buffer_size_configured =
                self.min_buffer_size_in_items.is_some() || dest.min_buffer_size_in_items.is_some();

            min_bytes = if buffer_size_configured {
                let min_self = self.min_buffer_size_in_items.unwrap_or(0);
                let min_reader = dest.min_buffer_size_in_items.unwrap_or(0);
                std::cmp::max(
                    min_bytes,
                    std::cmp::max(min_self, min_reader) * size_of::<D>(),
                )
            } else {
                std::cmp::max(min_bytes, futuresdr::runtime::config::config().buffer_size)
            };

            while (buffer_size < min_bytes) || (buffer_size % size_of::<D>() != 0) {
                buffer_size += page_size;
            }

            self.writer = Some(generic::Circular::with_capacity(buffer_size).unwrap());
        } else if dest.min_items.is_some() || dest.min_buffer_size_in_items.is_some() {
            warn!("circular buffer is already created, size constraints of reader are not considered.");
        }

        let writer_notifier = MyNotifier {
            sender: self.inbox.clone(),
        };

        let reader_notifier = MyNotifier {
            sender: dest.inbox.clone(),
        };

        let reader = self
            .writer
            .as_mut()
            .unwrap()
            .add_reader(reader_notifier, writer_notifier);

        self.readers
            .push((dest.port_id.clone(), dest.inbox.clone()));

        dest.reader = Some(reader);
        dest.writer_output_id = self.port_id.clone();
        dest.writer_inbox = self.inbox.clone();
    }
    async fn notify_finished(&mut self) {
        for i in self.readers.iter_mut() {
            let _ =
                i.1.send(BlockMessage::StreamInputDone {
                    input_id: i.0.clone(),
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

impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Item = D;

    fn produce(&mut self, items: usize) {
        self.writer
            .as_mut()
            .unwrap()
            .produce(items, std::mem::take(&mut self.tags));
    }
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        let s = self.writer.as_mut().unwrap().slice(false);
        (s, Tags::new(&mut self.tags, 0))
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
}

impl<D> fmt::Debug for Writer<D>
where
    D: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("output_id", &self.port_id)
            .field("finished", &self.finished)
            .finish()
    }
}

/// Circular Reader
pub struct Reader<D>
where
    D: CpuSample,
{
    reader: Option<generic::Reader<D, MyNotifier, MyMetadata>>,
    finished: bool,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    block_id: BlockId,
    port_id: PortId,
    inbox: Sender<BlockMessage>,
    tags: Vec<ItemTag>,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<D> Default for Reader<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        let (rx, _) = channel(0);
        Self {
            reader: None,
            finished: false,
            writer_inbox: rx.clone(),
            writer_output_id: PortId::default(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inbox: rx,
            tags: vec![],
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }
}

#[async_trait]
impl<D> BufferReader for Reader<D>
where
    D: CpuSample,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
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

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if let Some((s, tags)) = self.reader.as_mut().unwrap().slice(false) {
            self.tags = tags;
            (s, &self.tags)
        } else {
            debug_assert!(self.tags.is_empty());
            (&[], &self.tags)
        }
    }
    fn consume(&mut self, amount: usize) {
        self.reader.as_mut().unwrap().consume(amount);
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
}

impl<D> fmt::Debug for Reader<D>
where
    D: CpuSample,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Reader")
            .field("writer_output_id", &self.writer_output_id)
            .field("finished", &self.finished)
            .finish()
    }
}
