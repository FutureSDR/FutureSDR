use futures::prelude::*;
use std::fmt;
use vmcircbuffer::generic;

use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
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
pub struct Writer<D: Default + Send + Sync> {
    _min_bytes: Option<usize>,
    _min_items: Option<usize>,
    inbox: Option<Sender<BlockMessage>>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    writer: Option<generic::Writer<D, MyNotifier, MyMetadata>>,
    readers: Vec<(PortId, Sender<BlockMessage>)>,
    finished: bool,
}

impl<D: Default + Send + Sync> Writer<D> {
    fn new() -> Self {
        Self {
            _min_bytes: None,
            _min_items: None,
            inbox: None,
            block_id: None,
            port_id: None,
            writer: None,
            readers: vec![],
            finished: false,
        }
    }
}

impl<D: Default + Send + Sync> Default for Writer<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Default + Send + Sync> BufferWriter for Writer<D> {
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
    }
    fn connect(&mut self, _dest: &mut Self::Reader) {
        // let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
        // let mut buffer_size = page_size;
        //
        // let item_size = std::mem::size_of::<D>();
        // while (buffer_size < min_bytes) || (buffer_size % item_size != 0) {
        //     buffer_size += page_size;
        // }
        //
        // WriterInner {
        //     writer: generic::Circular::with_capacity(buffer_size).unwrap(),
        //     readers: Vec::new(),
        //     inbox,
        //     output_id,
        //     finished: false,
        // }
        todo!()
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

impl<D: Default + Send + Sync> CpuBufferWriter for Writer<D> {
    type Item = D;
    // fn add_reader(&mut self, inbox: Sender<BlockMessage>, input_id: usize) -> BufferReader {
    //     let writer_notifier = MyNotifier {
    //         sender: self.inbox.clone(),
    //     };
    //
    //     let reader_notifier = MyNotifier {
    //         sender: inbox.clone(),
    //     };
    //
    //     let reader = self.writer.add_reader(reader_notifier, writer_notifier);
    //
    //     self.readers.push((inbox, input_id));
    //
    //     BufferReader::Host(Box::new(Reader {
    //         reader,
    //         item_size: self.item_size,
    //         finished: false,
    //         writer_inbox: self.inbox.clone(),
    //         writer_output_id: self.output_id,
    //     }))
    // }

    fn produce(&mut self, items: usize) {
        self.writer.as_mut().unwrap().produce(items, vec![]);
    }

    fn produce_with_tags(&mut self, items: usize, tags: Vec<ItemTag>) {
        self.writer.as_mut().unwrap().produce(items, tags);
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        self.writer.as_mut().unwrap().slice(false)
    }
}

impl<D: Default + Send + Sync> fmt::Debug for Writer<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("output_id", &self.port_id)
            .field("finished", &self.finished)
            .finish()
    }
}

/// Circular Reader
#[derive(Default)]
pub struct Reader<D: Default + Send + Sync> {
    reader: Option<generic::Reader<D, MyNotifier, MyMetadata>>,
    finished: bool,
    writer_inbox: Option<Sender<BlockMessage>>,
    writer_output_id: Option<PortId>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    inbox: Option<Sender<BlockMessage>>,
}

impl<D: Default + Send + Sync> BufferReader for Reader<D> {
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
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

impl<D: Default + Send + Sync> CpuBufferReader for Reader<D> {
    type Item = D;

    fn slice(&mut self) -> &[Self::Item] {
        self.slice_with_tags().0
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], Vec<ItemTag>) {
        if let Some((s, tags)) = self.reader.as_mut().unwrap().slice(false) {
            (s, tags)
        } else {
            (&[], Vec::new())
        }
    }

    fn consume(&mut self, amount: usize) {
        self.reader.as_mut().unwrap().consume(amount);
    }
}

impl<D: Default + Send + Sync> fmt::Debug for Reader<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Reader")
            .field("writer_output_id", &self.writer_output_id)
            .field("finished", &self.finished)
            .finish()
    }
}
