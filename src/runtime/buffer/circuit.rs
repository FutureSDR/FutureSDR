use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::InplaceBuffer;
use crate::runtime::buffer::InplaceReader;
use crate::runtime::buffer::InplaceWriter;
use crate::runtime::buffer::Tags;
use crate::runtime::config::config;
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
    /// Create buffer
    fn with_items(items: usize) -> Self {
        Self {
            valid: 0,
            buffer: vec![T::default(); items].into_boxed_slice(),
            tags: Vec::new(),
        }
    }
}

impl<T> InplaceBuffer for Buffer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn set_valid(&mut self, valid: usize) {
        self.valid = valid;
    }

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
    inbound: Arc<Mutex<Vec<Option<Buffer<T>>>>>,
    outbound: Arc<Mutex<VecDeque<Buffer<T>>>>,
    buffer_size_in_items: usize,
    // for CPU buffer writer
    current: Option<Buffer<T>>,
    min_items: usize,
    min_buffer_size_in_items: Option<usize>,
    // dummy to return when no buffer available
    tags: Vec<ItemTag>,
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
            buffer_size_in_items: config().buffer_size / std::mem::size_of::<T>(),
            current: None,
            min_items: 1,
            min_buffer_size_in_items: None,
            tags: Vec::new(),
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
        if let Some(b) = self.current.take() {
            self.outbound.lock().unwrap().push_back(b);
        }
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
        self.inbound.lock().unwrap().pop().map(|b| {
            if let Some(mut b) = b {
                b.valid = b.buffer.len();
                b
            } else {
                Buffer::with_items(self.buffer_size_in_items)
            }
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.lock().unwrap().is_empty()
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let mut inbound = self.inbound.lock().unwrap();
        for _ in 0..n_buffers {
            inbound.push(Some(Buffer::with_items(n_items)));
        }
    }
}
impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            match self.inbound.lock().unwrap().pop() {
                Some(Some(mut b)) => {
                    b.valid = 0;
                    b.tags.clear();
                    self.current = Some(b);
                }
                Some(None) => {
                    self.current = Some(Buffer::with_items(self.buffer_size_in_items));
                }
                None => {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
            }
        }

        let c = self.current.as_mut().unwrap();
        (&mut c.buffer[c.valid..], Tags::new(&mut c.tags, c.valid))
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.buffer.len() - c.valid);
        c.valid += n;
        if (c.buffer.len() - c.valid) < self.min_items {
            let c = self.current.take().unwrap();
            self.outbound.lock().unwrap().push_back(c);

            let _ = self.reader_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again, if we have another buffer queued
            if !self.inbound.lock().unwrap().is_empty() {
                let _ = self.writer_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        self.min_items = std::cmp::max(self.min_items, n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.min_buffer_size_in_items = match self.min_buffer_size_in_items {
            Some(c) => Some(std::cmp::max(n, c)),
            None => Some(std::cmp::max(n, 1)),
        }
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for circuit writer");
        1
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
    circuit_start: Option<(Sender<BlockMessage>, Arc<Mutex<Vec<Option<Buffer<T>>>>>)>,
    finished: bool,
    // for CPU buffer reader
    current: Option<(Buffer<T>, usize)>,
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
            current: None,
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
            buffers.lock().unwrap().push(Some(buffer));
            let _ = inbox.try_send(BlockMessage::Notify);
        } else {
            warn!("Put empty buffer in unconnected circuit reader. Dropping buffer.")
        }
    }

    fn notify_consumed_buffer(&mut self) {
        if let Some((ref mut inbox, ref buffers)) = self.circuit_start {
            buffers.lock().unwrap().push(None);
            let _ = inbox.try_send(BlockMessage::Notify);
        } else {
            warn!("Dropped buffer in unconnected circuit reader. Dropping buffer.")
        }
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if self.current.is_none() {
            match self.inbound.lock().unwrap().pop_front() {
                Some(b) => {
                    self.current = Some((b, 0));
                }
                _ => {
                    static V: Vec<ItemTag> = vec![];
                    return (&[], &V);
                }
            }
        }

        let (c, o) = self.current.as_mut().unwrap();
        (&c.buffer[*o..c.valid], &c.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let (c, o) = self.current.as_mut().unwrap();
        debug_assert!(n <= c.valid - *o);
        *o += n;

        if *o == c.valid {
            let (b, _) = self.current.take().unwrap();
            match self.circuit_start {
                Some((ref mut inbox, ref queue)) => {
                    queue.lock().unwrap().push(Some(b));
                    let _ = inbox.try_send(BlockMessage::Notify);
                }
                None => {
                    warn!(
                        "circuit reader used as cpu buffer reader but not connected to circuit start. dropping buffer."
                    );
                }
            }

            // make sure to be called again, if we have another buffer queued
            if !self.inbound.lock().unwrap().is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not implemented for circuit reader");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not implemented for circuit reader");
    }

    fn max_items(&self) -> usize {
        warn!("max_items not implemented for circuit reader");
        1
    }
}
