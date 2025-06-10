use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::InplaceBuffer;
use crate::runtime::buffer::InplaceReader;
use crate::runtime::buffer::InplaceWriter;
use crate::runtime::config;
use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

/// In-place buffer
pub struct Buffer<T>
where
    T: CpuSample,
{
    valid: usize,
    buffer: Box<[T]>,
    tags: Vec<ItemTag>,
}

impl<T> Buffer<T>
where
    T: CpuSample,
{
    /// Set the number of valid items in the buffer
    pub fn new() -> Self {
        Self::with_items(config::config().buffer_size / std::mem::size_of::<T>())
    }
    /// Set the number of valid items in the buffer
    pub fn with_items(items: usize) -> Self {
        Self {
            valid: 0,
            buffer: vec![T::default(); items].into_boxed_slice(),
            tags: Vec::new(),
        }
    }
    /// Set the number of valid items in the buffer
    pub fn set_valid(&mut self, valid: usize) {
        self.valid = valid;
    }
}

impl<T> Default for Buffer<T>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> InplaceBuffer for Buffer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        &mut self.buffer[0..self.valid]
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        (&mut self.buffer[0..self.valid], &mut self.tags)
    }
}

/// Circuit Writer
pub struct Writer<T>
where
    T: CpuSample,
{
    reader_inbox: Sender<BlockMessage>,
    reader_input: PortId,
    writer_inbox: Sender<BlockMessage>,
    writer_id: BlockId,
    writer_output: PortId,
    inbound: Arc<Mutex<Vec<Buffer<T>>>>,
    outbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    /// Create circuit buffer writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            reader_inbox: rx.clone(),
            reader_input: PortId::default(),
            writer_inbox: rx,
            writer_id: BlockId::default(),
            writer_output: PortId::default(),
            inbound: Arc::new(Mutex::new(Vec::new())),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Close Circuit
    pub fn close_circuit(&mut self, end: &mut Reader<T>) {
        end.circuit_start = Some((self.writer_inbox.clone(), self.inbound.clone()));
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

impl<T> BufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.writer_id = block_id;
        self.writer_output = port_id;
        self.writer_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.reader_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.writer_id, self.writer_output
            )))
        } else {
            Ok(())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        self.reader_input = dest.reader_input.clone();
        self.reader_inbox = dest.reader_inbox.clone();
        self.outbound = dest.inbound.clone();

        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_output = self.writer_output.clone();
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output.clone()
    }
}

impl<T> InplaceWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    type Buffer = Buffer<T>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) {
        self.outbound.lock().unwrap().push_back(buffer);
        let _ = self.reader_inbox.try_send(BlockMessage::Notify);
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        self.inbound.lock().unwrap().pop().map(|mut b| {
            b.valid = b.buffer.len();
            b
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.lock().unwrap().is_empty()
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let mut inbound = self.inbound.lock().unwrap();
        for _ in 0..n_buffers {
            inbound.push(Buffer::with_items(n_items));
        }
    }
}

/// Circuit Reader
pub struct Reader<T>
where
    T: CpuSample,
{
    reader_inbox: Sender<BlockMessage>,
    reader_id: BlockId,
    reader_input: PortId,
    writer_inbox: Sender<BlockMessage>,
    writer_output: PortId,
    inbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    #[allow(clippy::type_complexity)]
    circuit_start: Option<(Sender<BlockMessage>, Arc<Mutex<Vec<Buffer<T>>>>)>,
    finished: bool,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    /// Create circuit buffer reader
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            reader_inbox: rx.clone(),
            reader_id: BlockId::default(),
            reader_input: PortId::default(),
            writer_inbox: rx,
            writer_output: PortId::default(),
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            circuit_start: None,
            finished: false,
        }
    }
}

impl<T> Default for Reader<T>
where
    T: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.reader_id = block_id;
        self.reader_input = port_id;
        self.reader_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.writer_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.reader_id, self.reader_input
            )))
        } else {
            Ok(())
        }
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output.clone(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        // Todo: also check for current buffer
        self.finished && self.inbound.lock().unwrap().is_empty()
    }

    fn block_id(&self) -> BlockId {
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input.clone()
    }
}

impl<T> InplaceReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    type Buffer = Buffer<T>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        self.inbound.lock().unwrap().pop_front()
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.lock().unwrap().is_empty()
    }

    fn put_empty_buffer(&mut self, buffer: Self::Buffer) {
        if let Some((ref mut inbox, ref buffers)) = self.circuit_start {
            buffers.lock().unwrap().push(buffer);
            let _ = inbox.try_send(BlockMessage::Notify);
        } else {
            warn!("Put empty buffer in non-connected circuit reader. Dropping buffer.")
        }
    }
}
