use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use crate::channel::mpsc::channel;
use crate::channel::mpsc::Sender;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;
use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::PortId;

#[derive(Debug)]
struct BufferEmpty<D: Send + Sync + 'static> {
    buffer: Box<[D]>,
}

#[derive(Debug)]
struct BufferFull<D: Send + Sync + 'static> {
    buffer: Box<[D]>,
    items: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct CurrentBuffer<D: Send + Sync + 'static> {
    buffer: Box<[D]>,
    offset: usize,
    capacity: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct State<D: Send + Sync + 'static> {
    writer_input: VecDeque<BufferEmpty<D>>,
    reader_input: VecDeque<BufferFull<D>>,
}

/// Slab writer
#[derive(Debug)]
pub struct Writer<D: Send + Sync + 'static> {
    current: Option<CurrentBuffer<D>>,
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_input_id: Option<usize>,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    finished: bool,
}

impl<D> Writer<D>
where D: Send + Sync + 'static {
    /// Create Slab writer
    pub fn new() -> Self {
        Self {
            current: None,
        }
        // let mut writer_input = VecDeque::new();
        // for _ in 0..2 {
        //     writer_input.push_back(BufferEmpty {
        //         buffer: vec![0; buffer_size].into_boxed_slice(),
        //     });
        // }
        //
        // Self {
        //     current: None,
        //     state: Arc::new(Mutex::new(State {
        //         writer_input,
        //         reader_input: VecDeque::new(),
        //     })),
        //     reserved_items,
        //     reader_inbox: None,
        //     reader_input_id: None,
        //     writer_inbox,
        //     writer_output_id,
        //     finished: false,
        // }
    }
}

impl<D> Default for Writer<D>
where D: Send + Sync + 'static {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> BufferWriter for Writer<D>
    where D: Send + Sync + 'static,
{
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        todo!()
    }

    fn validate(&self) -> Result<(), Error> {
        todo!()
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        todo!()
    }

    async fn notify_finished(&mut self) {
        todo!()
    }

    fn block_id(&self) -> futuresdr_types::BlockId {
        todo!()
    }

    fn port_id(&self) -> futuresdr_types::PortId {
        todo!()
    }
}

impl<D> CpuBufferWriter for Writer<D>
    where D: Send + Sync + 'static,
{
    type Item = D;

    fn slice(&mut self) -> &mut [Self::Item] {
        todo!()
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], super::Tags) {
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
    //     debug_assert!(self.reader_inbox.is_none());
    //     debug_assert!(self.reader_input_id.is_none());
    //
    //     self.reader_inbox = Some(reader_inbox.clone());
    //     self.reader_input_id = Some(reader_input_id);
    //
    //     BufferReader::Host(Box::new(Reader {
    //         current: None,
    //         state: self.state.clone(),
    //         item_size: self.item_size,
    //         reader_inbox,
    //         reserved_items: self.reserved_items,
    //         writer_inbox: self.writer_inbox.clone(),
    //         writer_output_id: self.writer_output_id,
    //         finished: false,
    //     }))
    // }
    //
    // fn bytes(&mut self) -> (*mut u8, usize) {
    //     if self.current.is_none() {
    //         let mut state = self.state.lock().unwrap();
    //         if let Some(b) = state.writer_input.pop_front() {
    //             let capacity = b.buffer.len() / self.item_size;
    //             self.current = Some(CurrentBuffer {
    //                 buffer: b.buffer,
    //                 offset: self.reserved_items,
    //                 capacity,
    //                 tags: Vec::new(),
    //             });
    //         } else {
    //             return (std::ptr::null_mut::<u8>(), 0);
    //         }
    //     }
    //
    //     let c = self.current.as_mut().unwrap();
    //
    //     unsafe {
    //         (
    //             c.buffer.as_mut_ptr().add(c.offset * self.item_size),
    //             (c.capacity - c.offset) * self.item_size,
    //         )
    //     }
    // }
    //
    // fn produce(&mut self, amount: usize, mut tags: Vec<ItemTag>) {
    //     debug_assert!(amount > 0);
    //
    //     let c = self.current.as_mut().unwrap();
    //     debug_assert!(amount <= c.capacity - c.offset);
    //     for t in tags.iter_mut() {
    //         t.index += c.offset;
    //     }
    //     c.tags.append(&mut tags);
    //     c.offset += amount;
    //     if c.offset == c.capacity {
    //         let c = self.current.take().unwrap();
    //         let mut state = self.state.lock().unwrap();
    //
    //         state.reader_input.push_back(BufferFull {
    //             buffer: c.buffer,
    //             items: c.capacity - self.reserved_items,
    //             tags: c.tags,
    //         });
    //
    //         let _ = self
    //             .reader_inbox
    //             .as_mut()
    //             .unwrap()
    //             .try_send(BlockMessage::Notify);
    //
    //         // make sure to be called again, if we have another buffer queued
    //         if !state.writer_input.is_empty() {
    //             let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    //         }
    //     }
    // }
    //
    // async fn notify_finished(&mut self) {
    //     if self.finished {
    //         return;
    //     }
    //
    //     if let Some(CurrentBuffer {
    //         buffer,
    //         offset,
    //         tags,
    //         ..
    //     }) = self.current.take()
    //     {
    //         if offset > self.reserved_items {
    //             let mut state = self.state.lock().unwrap();
    //
    //             state.reader_input.push_back(BufferFull {
    //                 buffer,
    //                 items: offset - self.reserved_items,
    //                 tags,
    //             });
    //         }
    //     }
    //
    //     let _ = self
    //         .reader_inbox
    //         .as_mut()
    //         .unwrap()
    //         .send(BlockMessage::StreamInputDone {
    //             input_id: self.reader_input_id.unwrap(),
    //         })
    //         .await;
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



/// Slab reader
#[derive(Debug)]
pub struct Reader<D: Send + Sync + 'static> {
    current: Option<CurrentBuffer<D>>,
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    reader_inbox: Sender<BlockMessage>,
    writer_inbox: Sender<BlockMessage>,
    finished: bool,
    block_id: BlockId,
    port_id: PortId,
    writer_output_id: PortId,
}

impl<D> Reader<D>
where D: Send + Sync + 'static {
    pub fn new() -> Self {
        let (reader_inbox, _) = channel(0);
        let (writer_inbox, _) = channel(0);
        Self {
            current: None,
            state: Arc::new(Mutex::new(State {
                writer_input: VecDeque::new(),
                reader_input: VecDeque::new(),
            })),
            reserved_items: 0,
            reader_inbox,
            writer_inbox,
            finished: false,
            block_id: BlockId(0),
            port_id: PortId("".to_string()),
            writer_output_id: PortId("".to_string()),
        }
    }
}

impl<D> Default for Reader<D> 
where D: Send + Sync + 'static {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<D> BufferReader for Reader<D> 
where D: Send + Sync + 'static {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.reader_inbox = inbox;
    }
    fn validate(&self) -> Result<(), Error> {
        if !self.reader_inbox.is_closed() {
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
    fn finished(&mut self) -> bool {
        self.finished && self.state.lock().unwrap().reader_input.is_empty()
    }
    fn block_id(&self) -> BlockId {
        self.block_id
    }
    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<D: Clone + Send + Sync + 'static> CpuBufferReader for Reader<D> {
    type Item = D;

    fn slice(&mut self) -> &[Self::Item] {
        if let Some(cur) = self.current.as_mut() {
            let left = cur.capacity - cur.offset;
            debug_assert!(left > 0);
            if left <= self.reserved_items {
                let mut state = self.state.lock().unwrap();
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = state.reader_input.pop_front()
                {

                    buffer[(self.reserved_items - left)..self.reserved_items].clone_from_slice(&cur.buffer[cur.offset..(cur.offset + left)]);
    
                    for t in tags.iter_mut() {
                        t.index += left;
                    }
                    cur.tags.append(&mut tags);
    
                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    state.writer_input.push_back(BufferEmpty { buffer: old });
                    let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    
                    cur.capacity = items + left;
                    cur.offset = self.reserved_items - left;
                }
            }
        } else {
            let mut state = self.state.lock().unwrap();
            if let Some(b) = state.reader_input.pop_front() {
                let capacity = b.items + self.reserved_items;
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    offset: self.reserved_items,
                    capacity,
                    tags: b.tags,
                });
            } else {
                return &[]
            }
        }
    
        let c = self.current.as_mut().unwrap();
        &c.buffer[c.offset..]
    }

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if let Some(cur) = self.current.as_mut() {
            let left = cur.capacity - cur.offset;
            debug_assert!(left > 0);
            if left <= self.reserved_items {
                let mut state = self.state.lock().unwrap();
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = state.reader_input.pop_front()
                {

                    buffer[(self.reserved_items - left)..self.reserved_items].clone_from_slice(&cur.buffer[cur.offset..(cur.offset + left)]);
    
                    for t in tags.iter_mut() {
                        t.index += left;
                    }
                    cur.tags.append(&mut tags);
    
                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    state.writer_input.push_back(BufferEmpty { buffer: old });
                    let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    
                    cur.capacity = items + left;
                    cur.offset = self.reserved_items - left;
                }
            }
        } else {
            let mut state = self.state.lock().unwrap();
            if let Some(b) = state.reader_input.pop_front() {
                let capacity = b.items + self.reserved_items;
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    offset: self.reserved_items,
                    capacity,
                    tags: b.tags,
                });
            } else {
                static V: Vec<ItemTag> = vec![];
                return (&[], &V)
            }
        }
    
        let c = self.current.as_mut().unwrap();
        (&c.buffer[c.offset..], &c.tags)
    }

    fn consume(&mut self, n: usize) {
        debug_assert!(n > 0);
    
        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.capacity - c.offset);
        c.offset += n;
    
        if c.offset == c.capacity {
            let b = self.current.take().unwrap();
            let mut state = self.state.lock().unwrap();
    
            state
                .writer_input
                .push_back(BufferEmpty { buffer: b.buffer });
    
            let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    
            // make sure to be called again, if we have another buffer queued
            if !state.reader_input.is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        } else if c.capacity - c.offset <= self.reserved_items {
            let state = self.state.lock().unwrap();
            if !state.reader_input.is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        }
    }
}
