use futures::channel::mpsc::Sender;
use std::any::Any;
use std::fmt::Debug;
use std::usize;

use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;

/// Buffer Builder
///
/// Used for [`connect_with_type`](crate::runtime::Flowgraph::connect_stream_with_type) calls.
pub trait BufferBuilder: Send + Sync + Any {
    /// Build the buffer, creating the writer
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter;
}

/// CPU buffer writer
#[async_trait]
pub trait BufferWriterHost: Send + Any + Debug {
    /// Add a reader
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader;
    /// Cast to any
    fn as_any(&mut self) -> &mut dyn Any;
    /// Produce samples
    fn produce(&mut self, amount: usize, tags: Vec<ItemTag>);
    /// Get buffer
    fn bytes(&mut self) -> (*mut u8, usize);
    /// Notify readers that we are finished
    async fn notify_finished(&mut self);
    /// Mark as finished
    fn finish(&mut self);
    /// Check, if we are marked as finished
    fn finished(&self) -> bool;
}

/// Custom buffer writer
#[async_trait]
pub trait BufferWriterCustom: Send + Any + Debug {
    /// Add a reader
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader;
    /// Cast to any
    fn as_any(&mut self) -> &mut dyn Any;
    /// Notify readers that we are finished
    async fn notify_finished(&mut self);
    /// Mark as finished
    fn finish(&mut self);
    /// Check, if we are marked as finished
    fn finished(&self) -> bool;
}

/// Buffer writer
#[derive(Debug)]
pub enum BufferWriter {
    /// CPU implementation
    Host(Box<dyn BufferWriterHost>),
    /// Custom buffer for use with accelerators
    Custom(Box<dyn BufferWriterCustom>),
}

impl BufferWriter {
    /// Add a reader
    pub fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        match self {
            BufferWriter::Host(w) => w.add_reader(reader_inbox, reader_input_id),
            BufferWriter::Custom(w) => w.add_reader(reader_inbox, reader_input_id),
        }
    }
    /// Try to cast to given type
    pub fn try_as<W: 'static>(&mut self) -> Option<&mut W> {
        match self {
            BufferWriter::Host(w) => w.as_any().downcast_mut::<W>(),
            BufferWriter::Custom(w) => w.as_any().downcast_mut::<W>(),
        }
    }
    /// Produce samples
    pub fn produce(&mut self, amount: usize, tags: Vec<ItemTag>) {
        match self {
            BufferWriter::Host(w) => w.produce(amount, tags),
            _ => unimplemented!(),
        }
    }
    /// Get buffer
    pub fn bytes(&mut self) -> (*mut u8, usize) {
        match self {
            BufferWriter::Host(w) => w.bytes(),
            _ => unimplemented!(),
        }
    }
    /// Notify readers that we are finished
    pub async fn notify_finished(&mut self) {
        match self {
            BufferWriter::Host(w) => w.notify_finished().await,
            BufferWriter::Custom(w) => w.notify_finished().await,
        }
    }
    /// Mark as finished
    pub fn finish(&mut self) {
        match self {
            BufferWriter::Host(w) => w.finish(),
            BufferWriter::Custom(w) => w.finish(),
        }
    }
    /// Check, if we are marked as finished
    pub fn finished(&self) -> bool {
        match self {
            BufferWriter::Host(w) => w.finished(),
            BufferWriter::Custom(w) => w.finished(),
        }
    }
}

/// CPU buffer reader
#[async_trait]
pub trait BufferReaderHost: Send + Any + Debug {
    /// Cast to any
    fn as_any(&mut self) -> &mut dyn Any;

    /// Get buffer
    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>);

    /// Consume samples
    fn consume(&mut self, amount: usize);

    /// Notify writers that we are finished
    async fn notify_finished(&mut self);

    /// Mark as finished
    fn finish(&mut self);

    /// Check, if we are marked as finished
    fn finished(&self) -> bool;
}

/// Custom buffer reader
#[async_trait]
pub trait BufferReaderCustom: Send + Any + Debug {
    /// Cast to any
    fn as_any(&mut self) -> &mut dyn Any;
    /// Notify writers that we are finished
    async fn notify_finished(&mut self);
    /// Mark as finished
    fn finish(&mut self);
    /// Check, if we are marked as finished
    fn finished(&self) -> bool;
}

/// Buffer reader
#[derive(Debug)]
pub enum BufferReader {
    /// CPU implementation
    Host(Box<dyn BufferReaderHost>),
    /// Custom buffer for use with accelerators
    Custom(Box<dyn BufferReaderCustom>),
}

impl BufferReader {
    /// Get buffer
    pub fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        match self {
            BufferReader::Host(w) => w.bytes(),
            _ => unimplemented!(),
        }
    }
    /// Consume samples
    pub fn consume(&mut self, amount: usize) {
        match self {
            BufferReader::Host(w) => w.consume(amount),
            _ => unimplemented!(),
        }
    }
    /// Try to cast to given type
    pub fn try_as<W: 'static>(&mut self) -> Option<&mut W> {
        match self {
            BufferReader::Host(w) => w.as_any().downcast_mut::<W>(),
            BufferReader::Custom(w) => w.as_any().downcast_mut::<W>(),
        }
    }
    /// Notify writers that we are finished
    pub async fn notify_finished(&mut self) {
        match self {
            BufferReader::Host(w) => w.notify_finished().await,
            BufferReader::Custom(w) => w.notify_finished().await,
        }
    }
    /// Mark as finished
    pub fn finish(&mut self) {
        match self {
            BufferReader::Host(w) => w.finish(),
            BufferReader::Custom(w) => w.finish(),
        }
    }
    /// Check, if we are marked as finished
    pub fn finished(&self) -> bool {
        match self {
            BufferReader::Host(w) => w.finished(),
            BufferReader::Custom(w) => w.finished(),
        }
    }
}
