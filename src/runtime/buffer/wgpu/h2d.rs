use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::wgpu::InputBufferEmpty as BufferEmpty;
use crate::runtime::buffer::wgpu::InputBufferFull as BufferFull;

#[derive(Debug)]
struct CurrentBuffer<D>
where
    D: CpuSample,
{
    buffer: Box<[D]>,
    item_offset: usize,
}

// ====================== WRITER ============================
/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    current: Option<CurrentBuffer<D>>,
    inbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    writer_id: BlockId,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    reader_input_id: PortId,
    tags: Vec<ItemTag>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            writer_id: BlockId::default(),
            writer_inbox: rx.clone(),
            writer_output_id: PortId::default(),
            reader_inbox: rx,
            reader_input_id: PortId::default(),
            tags: Vec::new(),
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
        self.writer_id = block_id;
        self.writer_output_id = port_id;
        self.writer_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.reader_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.writer_id, self.writer_output_id
            )))
        } else {
            Ok(())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();
        self.reader_input_id = dest.reader_input_id.clone();
        self.reader_inbox = dest.reader_inbox.clone();
        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_output_id = self.writer_output_id.clone();
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");
        if let Some(CurrentBuffer {
            item_offset,
            buffer,
        }) = self.current.take()
        {
            if item_offset > 0 {
                self.outbound.lock().unwrap().push_back(BufferFull {
                    buffer,
                    n_items: item_offset,
                });
            }
        }

        self.reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await
            .unwrap();
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output_id.clone()
    }
}

#[async_trait]
impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    item_offset: 0,
                });
            } else {
                debug!("H2D writer called bytes, buff is none");
                return (&mut [], Tags::new(&mut self.tags, 0));
            }
        }

        let current = self.current.as_mut().unwrap();

        (
            &mut current.buffer[current.item_offset..],
            Tags::new(&mut self.tags, 0),
        )
    }

    fn produce(&mut self, amount: usize) {
        debug!("H2D writer called produce {}", amount);
        let current = self.current.as_mut().unwrap();
        let item_capacity = current.buffer.len();

        debug_assert!(amount + current.item_offset <= item_capacity);
        current.item_offset += amount;
        if current.item_offset == item_capacity {
            let buffer = self.current.take().unwrap().buffer;
            self.outbound.lock().unwrap().push_back(BufferFull {
                buffer,
                n_items: item_capacity,
            });

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    item_offset: 0,
                });
            }

            let _ = self.reader_inbox.try_send(BlockMessage::Notify);
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for wgpu buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for wgpu buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for wgpu buffers");
        usize::MAX
    }
}

// ====================== READER ============================
/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
    inbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    reader_id: BlockId,
    reader_input_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    writer_inbox: Sender<BlockMessage>,
    finished: bool,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Send empty buffer back to writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            reader_id: BlockId::default(),
            reader_input_id: PortId::default(),
            reader_inbox: rx.clone(),
            writer_output_id: PortId::default(),
            writer_inbox: rx,
            finished: false,
        }
    }

    /// Send empty buffer back to writer
    pub fn submit(&mut self, buffer: BufferEmpty<D>) {
        debug!("H2D reader handling empty buffer");
        self.outbound.lock().unwrap().push(buffer);
        let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    }

    /// Get full buffer
    pub fn get_buffer(&mut self) -> Option<BufferFull<D>> {
        let mut vec = self.inbound.lock().unwrap();
        vec.pop_front()
    }
}

impl<D> Default for Reader<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
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
        self.reader_id = block_id;
        self.reader_input_id = port_id;
        self.reader_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.reader_id, self.reader_input_id
            )))
        } else {
            Ok(())
        }
    }

    async fn notify_finished(&mut self) {
        debug!("H2D reader finish");
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id.clone(),
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }

    fn block_id(&self) -> BlockId {
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input_id.clone()
    }
}
