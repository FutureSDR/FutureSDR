use futures::prelude::*;
use std::sync::Arc;
use std::sync::Mutex;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::Subbuffer;

use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::vulkan::Buffer;
use crate::runtime::Error;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::PortId;
use crate::runtime::buffer::Tags;

// everything is measured in items, e.g., offsets, capacity, space available

// ====================== WRITER ============================
/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<T: BufferContents> {
    current: Option<Buffer<T>>,
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    finished: bool,
    inbox: Option<Sender<BlockMessage>>,
    block_id: Option<BlockId>,
    port_id: Option<PortId>,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_port_id: Option<PortId>,
}

impl<T> Writer<T>
where
    T: BufferContents,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            finished: false,
            inbox: None,
            block_id: None,
            port_id: None,
            reader_inbox: None,
            reader_port_id: None,
        }
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

    fn init(
        &mut self,
        block_id: BlockId,
        port_id: PortId,
        inbox: Sender<BlockMessage>,
    ) {
        self.block_id = Some(block_id);
        self.port_id = Some(port_id);
        self.inbox = Some(inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.reader_inbox.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();
        self.reader_port_id = dest.port_id.clone();
        self.reader_inbox = dest.inbox.clone();
        dest.writer_inbox = self.inbox.clone();
        dest.writer_port_id = self.port_id.clone();
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");
    
        if let Some(buffer) = self.current.take() {
            if buffer.offset > 0 {
                self.outbound.lock().unwrap().push(buffer);
            }
        }
    
        self.reader_inbox
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_port_id.clone().unwrap(),
            })
            .await
            .unwrap();
    }

    fn block_id(&self) -> BlockId {
        self.block_id.unwrap()
    }

    fn port_id(&self) -> PortId {
        self.port_id.as_ref().unwrap().clone()
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: BufferContents,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        todo!()
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        todo!()
    }

    fn produce(&mut self, n: usize) {
        todo!()
    }

    // fn add_reader(
    //     &mut self,
    //     reader_inbox: Sender<BlockMessage>,
    //     reader_input_id: usize,
    // ) -> BufferReader {
    //     debug!("H2D writer called add reader");
    //     debug_assert!(self.reader_inbox.is_none());
    //     debug_assert!(self.reader_input_id.is_none());
    //
    //     self.reader_inbox = Some(reader_inbox);
    //     self.reader_input_id = Some(reader_input_id);
    //
    //     debug_assert_eq!(reader_input_id, 0);
    //     BufferReader::Custom(Box::new(Reader {
    //         inbound: self.outbound.clone(),
    //         outbound: self.inbound.clone(),
    //         writer_inbox: self.writer_inbox.clone(),
    //         writer_output_id: self.writer_output_id,
    //         finished: false,
    //     }))
    // }
    //
    // fn as_any(&mut self) -> &mut dyn Any {
    //     self
    // }
    //
    // fn bytes(&mut self) -> (*mut u8, usize) {
    //     if self.buffer.is_none() {
    //         if let Some(b) = self.inbound.lock().unwrap().pop() {
    //             self.buffer = Some(CurrentBuffer {
    //                 buffer: b,
    //                 offset: 0,
    //             });
    //         } else {
    //             // debug!("H2D writer called bytes, buff is none");
    //             return (std::ptr::null_mut::<u8>(), 0);
    //         }
    //     }
    //
    //     // debug!("H2D writer called bytes, buff is some");
    //     unsafe {
    //         let buffer = self.buffer.as_mut().unwrap();
    //         let capacity = buffer.buffer.buffer.size() as usize / self.item_size;
    //         let mut ret = buffer.buffer.buffer.write().unwrap();
    //         (
    //             ret.as_mut_ptr().add(buffer.offset * self.item_size),
    //             (capacity - buffer.offset) * self.item_size,
    //         )
    //     }
    // }
    //
    // fn produce(&mut self, amount: usize, _tags: Vec<ItemTag>) {
    //     // debug!("H2D writer called produce {}", amount);
    //     let buffer = self.buffer.as_mut().unwrap();
    //     let capacity = buffer.buffer.buffer.size() as usize / self.item_size;
    //
    //     debug_assert!(amount + buffer.offset <= capacity);
    //     buffer.offset += amount;
    //     if buffer.offset == capacity {
    //         let buffer = self.buffer.take().unwrap().buffer.buffer;
    //         self.outbound.lock().unwrap().push(BufferFull {
    //             buffer,
    //             used_bytes: capacity * self.item_size,
    //         });
    //
    //         if let Some(b) = self.inbound.lock().unwrap().pop() {
    //             self.buffer = Some(CurrentBuffer {
    //                 buffer: b,
    //                 offset: 0,
    //             });
    //         }
    //
    //         let _ = self
    //             .reader_inbox
    //             .as_mut()
    //             .unwrap()
    //             .try_send(BlockMessage::Notify);
    //     }
    // }
    //
    // async fn notify_finished(&mut self) {
    // }
}

// ====================== READER ============================
/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<T: BufferContents> {
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    outbound: Arc<Mutex<Vec<Buffer<T>>>>,
    inbox: Option<Sender<BlockMessage>>,
    port_id: Option<PortId>,
    writer_port_id: Option<PortId>,
    writer_inbox: Option<Sender<BlockMessage>>,
    finished: bool,
}

impl<T> Reader<T>
where
    T: BufferContents,
{
    /// Create a Reader
    pub fn new() -> Self {
        Self {
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            inbox: None,
            port_id: None,
            writer_port_id: None,
            writer_inbox: None,
            finished: false,
        }
    }

    /// Send empty buffer back to writer
    pub fn submit(&mut self, buffer: Subbuffer<[T]>) {
        // debug!("H2D reader handling empty buffer");
        self.outbound.lock().unwrap().push(buffer);
        let _ = self.writer_inbox.as_mut().unwrap().try_send(BlockMessage::Notify);
    }

    /// Get full buffer
    pub fn buffers(&mut self) -> Vec<Subbuffer<[T]>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
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
    // async fn notify_finished(&mut self) {
    //     debug!("H2D reader finish");
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
    //     self.finished
    // }
}
