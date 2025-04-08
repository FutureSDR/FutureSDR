use futures::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::Subbuffer;

use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

// everything is measured in items, e.g., offsets, capacity, space available

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents> {
    inbound: Arc<Mutex<Vec<Subbuffer<[T]>>>>,
    outbound: Arc<Mutex<VecDeque<Subbuffer<[T]>>>>,
    finished: bool,
    writer_inbox: Option<Sender<BlockMessage>>,
    writer_output_id: usize,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_input_id: Option<usize>,
}

impl<T> Writer<T>
where
    T: BufferContents,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            finished: false,
            writer_inbox: None,
            writer_output_id: 0,
            reader_inbox: None,
            reader_input_id: None,
        }
    }

    /// All available empty buffers
    pub fn buffers(&mut self) -> Vec<Subbuffer<[T]>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: Subbuffer<[T]>) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self
            .reader_inbox
            .as_mut()
            .unwrap()
            .try_send(BlockMessage::Notify);
    }
}

impl<T> Default for Writer<T>
where
    T: BufferContents,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BufferWriter for Writer<T>
where
    T: BufferContents,
{
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        todo!()
    }

    fn validate(&self) -> Result<(), Error> {
        todo!()
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        todo!()
    }

    fn notify_finished(&mut self) -> impl Future<Output = ()> + Send {
        async { todo!() }
    }

    fn block_id(&self) -> BlockId {
        todo!()
    }

    fn port_id(&self) -> PortId {
        todo!()
    }
    // fn add_reader(
    //     &mut self,
    //     reader_inbox: Sender<BlockMessage>,
    //     reader_input_id: usize,
    // ) -> BufferReader {
    //     debug_assert!(self.reader_inbox.is_none());
    //     debug_assert!(self.reader_input_id.is_none());
    //
    //     self.reader_inbox = Some(reader_inbox.clone());
    //     self.reader_input_id = Some(reader_input_id);
    //
    //     BufferReader::Host(Box::new(Reader {
    //         buffer: None,
    //         outbound: self.inbound.clone(),
    //         inbound: self.outbound.clone(),
    //         item_size: self.item_size,
    //         writer_inbox: self.writer_inbox.clone(),
    //         writer_output_id: self.writer_output_id,
    //         my_inbox: reader_inbox,
    //         finished: false,
    //     }))
    // }
    //
    // fn as_any(&mut self) -> &mut dyn Any {
    //     self
    // }
    //
    // async fn notify_finished(&mut self) {
    //     if self.finished {
    //         return;
    //     }
    //
    //     self.reader_inbox
    //         .as_mut()
    //         .unwrap()
    //         .send(BlockMessage::StreamInputDone {
    //             input_id: self.reader_input_id.unwrap(),
    //         })
    //         .await
    //         .unwrap();
    // }
    //
    // fn finish(&mut self) {
    //     self.finished = true;
    // }
    //
    // fn finished(&self) -> bool {
    //     self.finished
    // }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents> {
    current: Option<Subbuffer<[T]>>,
    inbound: Arc<Mutex<VecDeque<Subbuffer<[T]>>>>,
    outbound: Arc<Mutex<Vec<Subbuffer<[T]>>>>,
    writer_inbox: Option<Sender<BlockMessage>>,
    writer_output_id: usize,
    my_inbox: Option<Sender<BlockMessage>>,
    finished: bool,
}
impl<T> Reader<T>
where
    T: BufferContents,
{
    pub fn new() -> Self {
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            writer_inbox: None,
            writer_output_id: 0,
            my_inbox: None,
            finished: false,
        }
    }
}

impl<T> Default for Reader<T>
where
    T: BufferContents,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BufferReader for Reader<T>
where
    T: BufferContents,
{
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        todo!()
    }

    fn validate(&self) -> Result<(), Error> {
        todo!()
    }

    fn notify_finished(&mut self) -> impl Future<Output = ()> + Send {
        async { todo!() }
    }

    fn finish(&mut self) {
        todo!()
    }

    fn finished(&mut self) -> bool {
        todo!()
    }

    fn block_id(&self) -> BlockId {
        todo!()
    }

    fn port_id(&self) -> PortId {
        todo!()
    }
    // fn as_any(&mut self) -> &mut dyn Any {
    //     self
    // }
    //
    // fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
    //     if self.buffer.is_none() {
    //         if let Some(b) = self.inbound.lock().unwrap().pop_front() {
    //             self.buffer = Some(CurrentBuffer {
    //                 buffer: b,
    //                 offset: 0,
    //             });
    //         } else {
    //             return (std::ptr::null::<u8>(), 0, Vec::new());
    //         }
    //     }
    //
    //     unsafe {
    //         let buffer = self.buffer.as_ref().unwrap();
    //         let capacity = buffer.buffer.used_bytes / self.item_size;
    //         let ret = buffer.buffer.buffer.write().unwrap();
    //         (
    //             ret.as_ptr().add(buffer.offset * self.item_size),
    //             (capacity - buffer.offset) * self.item_size,
    //             Vec::new(),
    //         )
    //     }
    // }
    //
    // fn consume(&mut self, amount: usize) {
    //     debug_assert!(amount != 0);
    //     debug_assert!(self.buffer.is_some());
    //
    //     let buffer = self.buffer.as_mut().unwrap();
    //     let capacity = buffer.buffer.used_bytes / self.item_size;
    //
    //     debug_assert!(amount + buffer.offset <= capacity);
    //
    //     buffer.offset += amount;
    //     if buffer.offset == capacity {
    //         let buffer = self.buffer.take().unwrap().buffer.buffer;
    //         self.outbound.lock().unwrap().push(BufferEmpty { buffer });
    //         let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    //
    //         // make sure to be called again for another potentially
    //         // queued buffer. could also check if there is one and only
    //         // message in this case.
    //         let _ = self.my_inbox.try_send(BlockMessage::Notify);
    //     }
    // }
    //
    // async fn notify_finished(&mut self) {
    //     debug!("D2H Reader finish");
    //     if self.finished {
    //         return;
    //     }
    //
    //     self.writer_inbox
    //         .send(BlockMessage::StreamOutputDone {
    //             output_id: self.writer_output_id,
    //         })
    //         .await
    //         .unwrap();
    // }
    //
    // fn finish(&mut self) {
    //     self.finished = true;
    // }
    //
    // fn finished(&self) -> bool {
    //     self.finished && self.inbound.lock().unwrap().is_empty()
    // }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: BufferContents,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        todo!()
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        todo!()
    }

    fn consume(&mut self, n: usize) {
        todo!()
    }
}
