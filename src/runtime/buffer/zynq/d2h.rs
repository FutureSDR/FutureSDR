use futures::SinkExt;
use std::any::Any;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
use xilinx_dma::DmaBuffer;

use crate::channel::mpsc::channel;
use crate::channel::mpsc::Sender;
use crate::runtime::buffer::zynq::BufferEmpty;
use crate::runtime::buffer::zynq::BufferFull;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;

#[derive(Debug)]
struct CurrentBuffer {
    buffer: DmaBuffer,
    byte_offset: usize,
}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    writer_id: BlockId,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    reader_input_id: PortId,
    _p: PhantomData<D>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            writer_id: BlockId::default(),
            writer_inbox: rx.clone(),
            writer_output_id: PortId::default(),
            reader_inbox: rx,
            reader_input_id: PortId::default(),
            _p: PhantomData,
        }
    }

    /// All available empty buffers
    pub fn buffers(&mut self) -> Vec<BufferEmpty> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: BufferFull) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self.reader_inbox.try_send(BlockMessage::Notify);
    }
}

impl<T> Default for Writer<T>
where
    T: CpuSample,
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
        dest.writer_output_id = self.writer_output_id.clone();
        dest.writer_inbox = self.writer_inbox.clone();

        self.reader_inbox = dest.reader_inbox.clone();
        self.reader_input_id = dest.reader_input_id.clone();
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output_id.clone()
    }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
    current: Option<CurrentBuffer>,
    inbound: Arc<Mutex<VecDeque<BufferFull>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty>>>,
    writer_output_id: PortId,
    writer_inbox: Sender<BlockMessage>,
    reader_id: BlockId,
    reader_input_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    finished: bool,
    _p: PhantomData<D>,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create Vulkan Device-to-Host Reader
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            current: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            writer_output_id: PortId::default(),
            writer_inbox: rx.clone(),
            reader_id: BlockId::default(),
            reader_input_id: PortId::default(),
            reader_inbox: rx,
            finished: false,
            _p: PhantomData,
        }
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
        if self.finished {
            return;
        }

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
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input_id.clone()
    }
}

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        static V: Vec<ItemTag> = vec![];
        if self.current.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    byte_offset: 0,
                });
            } else {
                return (&[], &V);
            }
        }

        let current = self.current.as_mut().unwrap();

        unsafe {
            (
                std::slice::from_raw_parts(
                    (current.buffer.buffer() as *const u8).add(current.byte_offset) as *const D,
                    (current.buffer.size() - current.byte_offset) / size_of::<D>(),
                ),
                &V,
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        debug_assert!(self.current.is_some());

        let current = self.current.as_mut().unwrap();
        let byte_capacity = current.buffer.size() / size_of::<D>();

        debug_assert!(amount * size_of::<D>() + current.byte_offset <= byte_capacity);

        current.byte_offset += amount * size_of::<D>();
        if current.byte_offset == byte_capacity {
            let buffer = self.current.take().unwrap().buffer;
            self.outbound.lock().unwrap().push(BufferEmpty { buffer });
            let _ = self.writer_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again for another potentially
            // queued buffer. could also check if there is one and only
            // message in this case.
            let _ = self.reader_inbox.try_send(BlockMessage::Notify);
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for zynq buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for zynq buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for zynq buffers");
        usize::MAX
    }
}
