use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortId;
use futuresdr::runtime::buffer::BufferInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuSample;
use futuresdr::runtime::buffer::InplaceBuffer;
use futuresdr::runtime::buffer::InplaceReader;
use futuresdr::runtime::buffer::InplaceWriter;
use futuresdr::runtime::buffer::LocalBlockInbox;
use futuresdr::runtime::config::config;
use futuresdr::runtime::dev::ItemTag;
use futuresdr::runtime::dev::LocalBlockNotifier;

type Queue<T> = Rc<RefCell<VecDeque<Buffer<T>>>>;

fn queue<T>() -> Queue<T>
where
    T: CpuSample,
{
    Rc::new(RefCell::new(VecDeque::new()))
}

struct BufferStorage<T>
where
    T: CpuSample,
{
    valid: usize,
    buffer: Box<[T]>,
    tags: Vec<ItemTag>,
}

impl<T> BufferStorage<T>
where
    T: CpuSample,
{
    fn with_items(items: usize) -> Self {
        Self {
            valid: 0,
            buffer: vec![T::default(); items].into_boxed_slice(),
            tags: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.valid = 0;
        self.tags.clear();
    }
}

struct Origin<T>
where
    T: CpuSample,
{
    inbox: LocalBlockInbox,
    queue: Queue<T>,
}

/// Local-domain in-place buffer chunk.
pub struct Buffer<T>
where
    T: CpuSample,
{
    storage: Option<BufferStorage<T>>,
    origin: Option<Origin<T>>,
}

impl<T> Buffer<T>
where
    T: CpuSample,
{
    fn with_items(items: usize) -> Self {
        Self {
            storage: Some(BufferStorage::with_items(items)),
            origin: None,
        }
    }

    fn storage_mut(&mut self) -> &mut BufferStorage<T> {
        self.storage
            .as_mut()
            .expect("local in-place buffer storage missing")
    }

    fn arm(&mut self, inbox: LocalBlockInbox, queue: Queue<T>) {
        self.origin = Some(Origin { inbox, queue });
    }
}

impl<T> Drop for Buffer<T>
where
    T: CpuSample,
{
    fn drop(&mut self) {
        let Some(origin) = self.origin.take() else {
            return;
        };
        let Some(mut storage) = self.storage.take() else {
            return;
        };
        storage.reset();
        origin.queue.borrow_mut().push_back(Buffer {
            storage: Some(storage),
            origin: None,
        });
        origin.inbox.notify();
    }
}

impl<T> InplaceBuffer for Buffer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn set_valid(&mut self, valid: usize) {
        self.storage_mut().valid = valid;
    }

    fn slice(&mut self) -> &mut [Self::Item] {
        let storage = self.storage_mut();
        &mut storage.buffer[..storage.valid]
    }

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>) {
        let storage = self.storage_mut();
        (&mut storage.buffer[..storage.valid], &mut storage.tags)
    }
}

struct ConnectedWriter<T>
where
    T: CpuSample,
{
    reader_inbox: LocalBlockInbox,
    reader_input_id: PortId,
    reader_notifier: LocalBlockNotifier,
    outbound: Queue<T>,
}

/// Local-domain in-place writer.
pub struct Writer<T>
where
    T: CpuSample,
{
    inbox: LocalBlockInbox,
    block_id: BlockId,
    port_id: PortId,
    inbound: Queue<T>,
    connected: Option<ConnectedWriter<T>>,
}

impl<T> Writer<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            inbox: LocalBlockInbox::default(),
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inbound: queue(),
            connected: None,
        }
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
    type Inbox = LocalBlockInbox;
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: LocalBlockInbox) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.connected.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        let full = queue();
        self.connected = Some(ConnectedWriter {
            reader_inbox: dest.inbox.clone(),
            reader_input_id: dest.port_id.clone(),
            reader_notifier: dest.notifier.clone(),
            outbound: full.clone(),
        });
        dest.connected = Some(ConnectedReader {
            writer_inbox: self.inbox.clone(),
            writer_output_id: self.port_id.clone(),
            inbound: full,
        });
    }

    async fn notify_finished(&mut self) {
        let Some(connected) = &self.connected else {
            return;
        };
        let _ = connected
            .reader_inbox
            .stream_input_done(connected.reader_input_id.clone())
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> InplaceWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;
    type Buffer = Buffer<T>;

    fn put_full_buffer(&mut self, buffer: Self::Buffer) -> Result<(), Error> {
        let connected = self.connected.as_ref().expect("writer not connected");
        connected.outbound.borrow_mut().push_back(buffer);
        connected.reader_notifier.notify();
        Ok(())
    }

    fn get_empty_buffer(&mut self) -> Option<Self::Buffer> {
        self.inbound.borrow_mut().pop_back().map(|mut buffer| {
            let storage = buffer.storage_mut();
            storage.valid = storage.buffer.len();
            storage.tags.clear();
            buffer.arm(self.inbox.clone(), self.inbound.clone());
            buffer
        })
    }

    fn has_more_buffers(&mut self) -> bool {
        !self.inbound.borrow().is_empty()
    }

    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        for _ in 0..n_buffers {
            self.inbound
                .borrow_mut()
                .push_back(Buffer::with_items(n_items));
        }
    }

    fn inject_buffers(&mut self, n_buffers: usize) {
        let n_items = config().buffer_size / T::SIZE.get();
        self.inject_buffers_with_items(n_buffers, n_items);
    }
}

struct ConnectedReader<T>
where
    T: CpuSample,
{
    writer_inbox: LocalBlockInbox,
    writer_output_id: PortId,
    inbound: Queue<T>,
}

/// Local-domain in-place reader.
pub struct Reader<T>
where
    T: CpuSample,
{
    block_id: BlockId,
    port_id: PortId,
    inbox: LocalBlockInbox,
    notifier: LocalBlockNotifier,
    connected: Option<ConnectedReader<T>>,
    finished: bool,
}

impl<T> Reader<T>
where
    T: CpuSample,
{
    pub fn new() -> Self {
        Self {
            block_id: BlockId::default(),
            port_id: PortId::default(),
            inbox: LocalBlockInbox::default(),
            notifier: LocalBlockNotifier::default(),
            connected: None,
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

impl<T> BufferReader for Reader<T>
where
    T: CpuSample,
{
    type Inbox = LocalBlockInbox;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: LocalBlockInbox) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.notifier = inbox.notifier();
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.connected.is_some() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    async fn notify_finished(&mut self) {
        let Some(connected) = &self.connected else {
            return;
        };
        let _ = connected
            .writer_inbox
            .stream_output_done(connected.writer_output_id.clone())
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
            && self
                .connected
                .as_ref()
                .is_none_or(|connected| connected.inbound.borrow().is_empty())
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> InplaceReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;
    type Buffer = Buffer<T>;

    fn get_full_buffer(&mut self) -> Option<Self::Buffer> {
        self.connected
            .as_ref()
            .expect("reader not connected")
            .inbound
            .borrow_mut()
            .pop_front()
    }

    fn has_more_buffers(&mut self) -> bool {
        !self
            .connected
            .as_ref()
            .expect("reader not connected")
            .inbound
            .borrow()
            .is_empty()
    }
}
