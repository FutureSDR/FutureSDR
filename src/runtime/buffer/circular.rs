use futures::prelude::*;
use std::any::Any;
use std::fmt;
use vmcircbuffer::generic;

use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
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
pub struct Writer<D: Send + Sync + 'static> {
    min_bytes: usize,
    min_items: usize,
    inbox: Option<Sender<BlockMessage>>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    writer: Option<generic::Writer<D, MyNotifier, MyMetadata>>,
    readers: Vec<(PortId, Sender<BlockMessage>)>,
    finished: bool,
    tags: Vec<ItemTag>,
}

impl<D: Send + Sync + 'static> Writer<D> {
    fn new() -> Self {
        Self {
            min_bytes: 0,
            min_items: 0,
            inbox: None,
            block_id: None,
            port_id: None,
            writer: None,
            readers: vec![],
            finished: false,
            tags: vec![],
        }
    }
}

impl<D: Send + Sync + 'static> Default for Writer<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Send + Sync + 'static> BufferWriter for Writer<D> {
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
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

            let item_size = std::mem::size_of::<D>();
            while (buffer_size < self.min_bytes)
                || (buffer_size < self.min_items * item_size)
                || (buffer_size % item_size != 0)
            {
                buffer_size += page_size;
            }

            self.writer = Some(generic::Circular::with_capacity(buffer_size).unwrap());
        }

        let writer_notifier = MyNotifier {
            sender: self.inbox.as_ref().unwrap().clone(),
        };

        let reader_notifier = MyNotifier {
            sender: dest.inbox.as_ref().unwrap().clone(),
        };

        let reader = self
            .writer
            .as_mut()
            .unwrap()
            .add_reader(reader_notifier, writer_notifier);

        self.readers.push((
            dest.port_id.as_ref().unwrap().clone(),
            dest.inbox.as_ref().unwrap().clone(),
        ));

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
        self.block_id.unwrap()
    }
    fn port_id(&self) -> PortId {
        self.port_id.as_ref().unwrap().clone()
    }
}

impl<D: Send + Sync> CpuBufferWriter for Writer<D> {
    type Item = D;

    fn produce(&mut self, items: usize) {
        self.writer
            .as_mut()
            .unwrap()
            .produce(items, std::mem::take(&mut self.tags));
    }
    fn slice(&mut self) -> &mut [Self::Item] {
        self.writer.as_mut().unwrap().slice(false)
    }
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        let s = self.writer.as_mut().unwrap().slice(false);
        (s, Tags::new(&mut self.tags, 0))
    }
}

impl<D: Send + Sync> fmt::Debug for Writer<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("output_id", &self.port_id)
            .field("finished", &self.finished)
            .finish()
    }
}

/// Circular Reader
pub struct Reader<D: Send + Sync> {
    reader: Option<generic::Reader<D, MyNotifier, MyMetadata>>,
    finished: bool,
    writer_inbox: Option<Sender<BlockMessage>>,
    writer_output_id: Option<PortId>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    inbox: Option<Sender<BlockMessage>>,
    tags: Vec<ItemTag>,
}

impl<D: Send + Sync + 'static> Default for Reader<D> {
    fn default() -> Self {
        Self {
            reader: None,
            finished: false,
            writer_inbox: None,
            writer_output_id: None,
            block_id: None,
            port_id: None,
            inbox: None,
            tags: vec![],
        }
    }
}

#[async_trait]
impl<D: Send + Sync + 'static> BufferReader for Reader<D> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
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
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id.as_ref().unwrap().clone(),
            })
            .await;
    }
    fn finish(&mut self) {
        self.finished = true;
    }
    fn finished(&mut self) -> bool {
        self.finished
    }
    fn block_id(&self) -> BlockId {
        self.block_id.unwrap()
    }
    fn port_id(&self) -> PortId {
        self.port_id.as_ref().unwrap().clone()
    }
}

impl<D: Send + Sync + 'static> CpuBufferReader for Reader<D> {
    type Item = D;

    fn slice(&mut self) -> &[Self::Item] {
        if let Some((s, tags)) = self.reader.as_mut().unwrap().slice(false) {
            self.tags = tags;
            s
        } else {
            debug_assert!(self.tags.is_empty());
            &[]
        }
    }
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
}

impl<D: Send + Sync> fmt::Debug for Reader<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Reader")
            .field("writer_output_id", &self.writer_output_id)
            .field("finished", &self.finished)
            .finish()
    }
}
