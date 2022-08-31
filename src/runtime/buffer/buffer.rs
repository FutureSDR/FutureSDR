use futures::channel::mpsc::Sender;
use std::any::Any;
use std::fmt::Debug;
use std::usize;

use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;

pub trait BufferBuilder: Send + Sync + Any {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter;
}

#[async_trait]
pub trait BufferWriterHost: Send + Any + Debug {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader;

    fn as_any(&mut self) -> &mut dyn Any;

    fn produce(&mut self, amount: usize, tags: Vec<ItemTag>);

    fn bytes(&mut self) -> (*mut u8, usize);

    async fn notify_finished(&mut self);

    fn finish(&mut self);

    fn finished(&self) -> bool;
}

#[async_trait]
pub trait BufferWriterCustom: Send + Any + Debug {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader;

    fn as_any(&mut self) -> &mut dyn Any;

    async fn notify_finished(&mut self);

    fn finish(&mut self);

    fn finished(&self) -> bool;
}

#[derive(Debug)]
pub enum BufferWriter {
    Host(Box<dyn BufferWriterHost>),
    Custom(Box<dyn BufferWriterCustom>),
}

impl BufferWriter {
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

    pub fn try_as<W: 'static>(&mut self) -> Option<&mut W> {
        match self {
            BufferWriter::Host(w) => w.as_any().downcast_mut::<W>(),
            BufferWriter::Custom(w) => w.as_any().downcast_mut::<W>(),
        }
    }

    pub fn produce(&mut self, amount: usize, tags: Vec<ItemTag>) {
        match self {
            BufferWriter::Host(w) => w.produce(amount, tags),
            _ => unimplemented!(),
        }
    }

    pub fn bytes(&mut self) -> (*mut u8, usize) {
        match self {
            BufferWriter::Host(w) => w.bytes(),
            _ => unimplemented!(),
        }
    }

    pub async fn notify_finished(&mut self) {
        match self {
            BufferWriter::Host(w) => w.notify_finished().await,
            BufferWriter::Custom(w) => w.notify_finished().await,
        }
    }

    pub fn finish(&mut self) {
        match self {
            BufferWriter::Host(w) => w.finish(),
            BufferWriter::Custom(w) => w.finish(),
        }
    }

    pub fn finished(&self) -> bool {
        match self {
            BufferWriter::Host(w) => w.finished(),
            BufferWriter::Custom(w) => w.finished(),
        }
    }
}

#[async_trait]
pub trait BufferReaderHost: Send + Any + Debug {
    fn as_any(&mut self) -> &mut dyn Any;

    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>);

    fn consume(&mut self, amount: usize);

    async fn notify_finished(&mut self);

    fn finish(&mut self);

    fn finished(&self) -> bool;
}

#[async_trait]
pub trait BufferReaderCustom: Send + Any + Debug {
    fn as_any(&mut self) -> &mut dyn Any;

    async fn notify_finished(&mut self);

    fn finish(&mut self);

    fn finished(&self) -> bool;
}

#[derive(Debug)]
pub enum BufferReader {
    Host(Box<dyn BufferReaderHost>),
    Custom(Box<dyn BufferReaderCustom>),
}

impl BufferReader {
    pub fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        match self {
            BufferReader::Host(w) => w.bytes(),
            _ => unimplemented!(),
        }
    }

    pub fn consume(&mut self, amount: usize) {
        match self {
            BufferReader::Host(w) => w.consume(amount),
            _ => unimplemented!(),
        }
    }

    pub fn try_as<W: 'static>(&mut self) -> Option<&mut W> {
        match self {
            BufferReader::Host(w) => w.as_any().downcast_mut::<W>(),
            BufferReader::Custom(w) => w.as_any().downcast_mut::<W>(),
        }
    }

    pub async fn notify_finished(&mut self) {
        match self {
            BufferReader::Host(w) => w.notify_finished().await,
            BufferReader::Custom(w) => w.notify_finished().await,
        }
    }

    pub fn finish(&mut self) {
        match self {
            BufferReader::Host(w) => w.finish(),
            BufferReader::Custom(w) => w.finish(),
        }
    }

    pub fn finished(&self) -> bool {
        match self {
            BufferReader::Host(w) => w.finished(),
            BufferReader::Custom(w) => w.finished(),
        }
    }
}
