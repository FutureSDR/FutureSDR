use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::any::Any;
use std::fmt;
use vmcircbuffer::generic;

use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::config;
use crate::runtime::AsyncMessage;
use crate::runtime::ItemTag;

// everything is measured in items, e.g., offsets, capacity, space available

struct MyNotifier {
    sender: Sender<AsyncMessage>,
}

impl generic::Notifier for MyNotifier {
    fn arm(&mut self) {}

    fn notify(&mut self) {
        let _ = self.sender.try_send(AsyncMessage::Notify);
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

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Circular {
    min_bytes: usize,
}

impl Eq for Circular {}

impl Circular {
    pub fn new() -> Circular {
        Circular {
            min_bytes: config::config().buffer_size,
        }
    }
    pub fn with_size(min_bytes: usize) -> Circular {
        Circular { min_bytes }
    }
}

impl Default for Circular {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for Circular {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        BufferWriter::Host(Box::new(Writer::new(
            item_size,
            self.min_bytes,
            writer_inbox,
            writer_output_id,
        )))
    }
}

pub struct Writer {
    writer: generic::Writer<u8, MyNotifier, MyMetadata>,
    readers: Vec<(Sender<AsyncMessage>, usize)>,
    item_size: usize,
    inbox: Sender<AsyncMessage>,
    output_id: usize,
    finished: bool,
}

impl Writer {
    pub fn new(
        item_size: usize,
        min_bytes: usize,
        inbox: Sender<AsyncMessage>,
        output_id: usize,
    ) -> Writer {
        let page_size = vmcircbuffer::double_mapped_buffer::pagesize();
        let mut buffer_size = page_size;

        while (buffer_size < min_bytes) || (buffer_size % item_size != 0) {
            buffer_size += page_size;
        }

        Writer {
            writer: generic::Circular::with_capacity(buffer_size).unwrap(),
            readers: Vec::new(),
            item_size,
            inbox,
            output_id,
            finished: false,
        }
    }
}

impl fmt::Debug for Writer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Writer")
            .field("item_size", &self.item_size)
            .field("output_id", &self.output_id)
            .field("finished", &self.finished)
            .finish()
    }
}

#[async_trait]
impl BufferWriterHost for Writer {
    fn add_reader(&mut self, inbox: Sender<AsyncMessage>, input_id: usize) -> BufferReader {
        let writer_notifier = MyNotifier {
            sender: self.inbox.clone(),
        };

        let reader_notifier = MyNotifier {
            sender: inbox.clone(),
        };

        let reader = self.writer.add_reader(reader_notifier, writer_notifier);

        self.readers.push((inbox, input_id));

        BufferReader::Host(Box::new(Reader {
            reader,
            item_size: self.item_size,
            finished: false,
            writer_inbox: self.inbox.clone(),
            writer_output_id: self.output_id,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn produce(&mut self, items: usize, mut tags: Vec<ItemTag>) {
        for t in tags.iter_mut() {
            t.index *= self.item_size;
        }
        self.writer.produce(items * self.item_size, tags);
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        let s = self.writer.slice(false);
        (s.as_mut_ptr(), s.len())
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        for i in self.readers.iter_mut() {
            let _ =
                i.0.send(AsyncMessage::StreamInputDone { input_id: i.1 })
                    .await;
        }
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for Writer {}
unsafe impl Sync for Writer {}

pub struct Reader {
    reader: generic::Reader<u8, MyNotifier, MyMetadata>,
    item_size: usize,
    finished: bool,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
}

#[async_trait]
impl BufferReaderHost for Reader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        if let Some((s, mut tags)) = self.reader.slice(false) {
            for t in tags.iter_mut() {
                t.index /= self.item_size;
            }
            (s.as_ptr(), s.len(), tags)
        } else {
            (std::ptr::null(), 0, Vec::new())
        }
    }

    fn consume(&mut self, amount: usize) {
        self.reader.consume(amount * self.item_size);
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let _ = self
            .writer_inbox
            .send(AsyncMessage::StreamOutputDone {
                output_id: self.writer_output_id,
            })
            .await;
        // note: maybe we need to drop the reader here
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("circular::Reader")
            .field("item_size", &self.item_size)
            .field("writer_output_id", &self.writer_output_id)
            .field("finished", &self.finished)
            .finish()
    }
}

unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}
