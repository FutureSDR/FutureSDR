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
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
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

// ====================== WRITER ============================
/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    current: Option<CurrentBuffer>,
    inbound: Arc<Mutex<Vec<BufferEmpty>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull>>>,
    writer_inbox: Sender<BlockMessage>,
    writer_id: BlockId,
    writer_output_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    reader_input_id: PortId,
    tags: Vec<ItemTag>,
    _p: PhantomData<D>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        debug!("H2D writer created");
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
            _p: PhantomData,
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
        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_output_id = self.writer_output_id.clone();

        self.reader_input_id = dest.reader_input_id.clone();
        self.reader_inbox = dest.reader_inbox.clone();
    }

    async fn notify_finished(&mut self) {
        debug!("H2D writer called finish");

        if let Some(CurrentBuffer {
            byte_offset,
            buffer,
        }) = self.current.take()
        {
            if byte_offset > 0 {
                self.outbound.lock().unwrap().push_back(BufferFull {
                    buffer,
                    used_bytes: byte_offset,
                });
            }
        }

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
                    byte_offset: 0,
                });
            } else {
                // debug!("H2D writer called bytes, buff is none");
                return (&mut [], Tags::new(&mut self.tags, 0));
            }
        }

        // debug!("H2D writer called bytes, buff is some");
        let current = self.current.as_mut().unwrap();

        unsafe {
            (
                std::slice::from_raw_parts_mut(
                    (current.buffer.buffer() as *mut u8).add(current.byte_offset) as *mut D,
                    (current.buffer.size() - current.byte_offset) / size_of::<D>(),
                ),
                Tags::new(&mut self.tags, 0),
            )
        }
    }

    fn produce(&mut self, n: usize) {
        // debug!("H2D writer called produce {}", amount);
        let current = self.current.as_mut().unwrap();
        let byte_capacity = current.buffer.size();

        debug_assert!(n * size_of::<D>() + current.byte_offset <= byte_capacity);
        current.byte_offset += n * size_of::<D>();
        if current.byte_offset == byte_capacity {
            let buffer = self.current.take().unwrap().buffer;
            self.outbound.lock().unwrap().push_back(BufferFull {
                buffer,
                used_bytes: byte_capacity,
            });

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    byte_offset: 0,
                });
            }

            let _ = self.reader_inbox.try_send(BlockMessage::Notify);
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for Vulkan buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for Vulkan buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for zynq buffers");
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
    inbound: Arc<Mutex<VecDeque<BufferFull>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty>>>,
    reader_id: BlockId,
    reader_input_id: PortId,
    reader_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    writer_inbox: Sender<BlockMessage>,
    finished: bool,
    _p: PhantomData<D>,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create a Reader
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
            _p: PhantomData,
        }
    }

    /// Send empty buffer back to writer
    pub fn submit(&mut self, buffer: BufferEmpty) {
        // debug!("H2D reader handling empty buffer");
        self.outbound.lock().unwrap().push(buffer);
        let _ = self.writer_inbox.try_send(BlockMessage::Notify);
    }

    /// Get full buffer
    pub fn get_buffer(&mut self) -> Option<BufferFull> {
        let mut vec = self.inbound.lock().unwrap();
        vec.pop_front()
    }

    /// Check, if a buffer is available
    pub fn buffer_available(&self) -> bool {
        let vec = self.inbound.lock().unwrap();
        !vec.is_empty()
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
