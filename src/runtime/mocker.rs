use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::channel::mpsc::channel;
use futuresdr_types::BlockId;
use std::any::Any;
use std::fmt::Debug;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::WorkIo;
use crate::runtime::WrappedKernel;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::config::config;

/// Mocker for a block
///
/// A harness to run a block without a runtime. Used for unit tests and benchmarking.
pub struct Mocker<K: Kernel> {
    /// Wrapped Block
    pub block: WrappedKernel<K>,
    message_sinks: Vec<Receiver<BlockMessage>>,
    messages: Vec<Vec<Pmt>>,
}

impl<K: KernelInterface + Kernel + 'static> Deref for Mocker<K> {
    type Target = WrappedKernel<K>;

    fn deref(&self) -> &Self::Target {
        &self.block
    }
}
impl<K: KernelInterface + Kernel + 'static> DerefMut for Mocker<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.block
    }
}

impl<K: KernelInterface + Kernel + 'static> Mocker<K> {
    /// Create mocker
    pub fn new(kernel: K) -> Self {
        let mut block = WrappedKernel::new(kernel, BlockId(0));
        let mut messages = Vec::new();
        let mut message_sinks = Vec::new();
        let msg_len = config().queue_size;

        for n in K::message_outputs() {
            messages.push(Vec::new());
            let (tx, rx) = channel(msg_len);
            message_sinks.push(rx);
            block
                .mio
                .connect(&PortId::new(*n), tx, &PortId::new("input"))
                .unwrap();
        }

        Mocker {
            block,
            message_sinks,
            messages,
        }
    }

    /// Post a PMT to a message handler of the block.
    pub fn post(&mut self, id: impl Into<PortId>, p: Pmt) -> Result<Pmt, Error> {
        let id = id.into();
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        let WrappedKernel {
            meta, mio, kernel, ..
        } = &mut self.block;
        async_io::block_on(kernel.call_handler(&mut io, mio, meta, id, p))
            .map_err(|e| Error::HandlerError(e.to_string()))
    }

    /// Run the block wrapped by the mocker
    pub fn run(&mut self) {
        crate::async_io::block_on(self.run_async());
    }

    /// Init the block wrapped by the mocker
    pub fn init(&mut self) {
        crate::async_io::block_on(async {
            self.block
                .kernel
                .init(&mut self.block.mio, &mut self.block.meta)
                .await
                .unwrap();
        });
    }

    /// Deinit the block wrapped by the mocker
    pub fn deinit(&mut self) {
        crate::async_io::block_on(async {
            self.block
                .kernel
                .deinit(&mut self.block.mio, &mut self.block.meta)
                .await
                .unwrap();
        });
    }

    /// Get produced PMTs from output message ports.
    pub fn messages(&self) -> Vec<Vec<Pmt>> {
        self.messages.clone()
    }

    /// Take produced PMTs from output message ports.
    pub fn take_messages(&mut self) -> Vec<Vec<Pmt>> {
        std::mem::take(&mut self.messages)
    }

    /// Run the mocker async
    pub async fn run_async(&mut self) {
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        loop {
            self.block
                .kernel
                .work(&mut io, &mut self.block.mio, &mut self.block.meta)
                .await
                .unwrap();

            for (n, r) in self.message_sinks.iter_mut().enumerate() {
                while let Ok(m) = r.try_recv() {
                    match m {
                        BlockMessage::Call { data, .. } => {
                            self.messages[n].push(data);
                        }
                        _ => panic!("Mocked Block produced unexpected BlockMessage {m:?}"),
                    }
                }
            }

            if !io.call_again {
                break;
            } else {
                io.call_again = false;
            }
        }
    }
}

#[derive(Debug)]
/// Buffer reader for Mocker
pub struct Reader<T: Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
    block_id: BlockId,
    port_id: PortId,
}

impl<T: Debug + Send + 'static> Reader<T> {
    /// Add input buffer with given data
    pub fn set(&mut self, data: Vec<T>)
    where
        T: Debug + Send + 'static,
    {
        self.set_with_tags(data, Vec::new());
    }

    /// Add input buffer with given data and tags
    pub fn set_with_tags(&mut self, data: Vec<T>, tags: Vec<ItemTag>)
    where
        T: Debug + Send + 'static,
    {
        self.data = data;
        self.tags = tags;
    }
}

impl<T: Debug + Send + 'static> Default for Reader<T> {
    fn default() -> Self {
        Self {
            data: vec![],
            tags: vec![],
            block_id: BlockId(0),
            port_id: PortId::new("input"),
        }
    }
}

#[async_trait]
impl<T: Debug + Send + 'static> BufferReader for Reader<T> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn init(&mut self, block_id: BlockId, port_id: PortId, _inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
    }
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
    async fn notify_finished(&mut self) {}
    fn finish(&mut self) {}
    fn finished(&self) -> bool {
        true
    }
    fn block_id(&self) -> BlockId {
        self.block_id
    }
    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> CpuBufferReader for Reader<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &[Self::Item] {
        self.data.as_slice()
    }
    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        (self.data.as_slice(), &self.tags)
    }
    fn consume(&mut self, n: usize) {
        self.data = self.data.split_off(n);
        self.tags.retain(|x| x.index >= n);

        for t in self.tags.iter_mut() {
            t.index -= n;
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items has no effect in with mocker");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items has no effect in a mocker");
    }

    fn max_items(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug)]
/// Stream buffer reader for Mocker
pub struct Writer<T: Clone + Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
    block_id: BlockId,
    port_id: PortId,
}

impl<T: Clone + Debug + Send + 'static> Default for Writer<T> {
    fn default() -> Self {
        Self {
            data: vec![],
            tags: vec![],
            block_id: BlockId(0),
            port_id: PortId::new("output"),
        }
    }
}

impl<T: Clone + Debug + Send + 'static> Writer<T> {
    /// Reserve space in the output buffer
    pub fn reserve(&mut self, n: usize) {
        self.data = Vec::with_capacity(n);
    }
    /// Get the data from the buffer (clone)
    pub fn get(&self) -> (Vec<T>, Vec<ItemTag>) {
        (self.data.clone(), self.tags.clone())
    }
    /// Take the data from the buffer
    pub fn take(&mut self) -> (Vec<T>, Vec<ItemTag>) {
        (
            std::mem::take(&mut self.data),
            std::mem::take(&mut self.tags),
        )
    }
}

impl<T: Clone + Debug + Send + 'static> BufferWriter for Writer<T> {
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, _inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
    }
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
    fn connect(&mut self, _dest: &mut Self::Reader) {}

    async fn notify_finished(&mut self) {}

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<T> CpuBufferWriter for Writer<T>
where
    T: CpuSample,
{
    type Item = T;

    fn slice(&mut self) -> &mut [Self::Item] {
        unsafe {
            std::slice::from_raw_parts_mut(
                self.data.as_mut_ptr().add(self.data.len()),
                self.data.capacity() - self.data.len(),
            )
        }
    }
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        let s = unsafe {
            std::slice::from_raw_parts_mut(
                self.data.as_mut_ptr().add(self.data.len()),
                self.data.capacity() - self.data.len(),
            )
        };
        (s, Tags::new(&mut self.tags, self.data.len()))
    }
    fn produce(&mut self, n: usize) {
        let curr_len = self.data.len();
        unsafe {
            self.data.set_len(curr_len + n);
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items has no effect in with mocker");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items has no effect in a mocker");
    }

    fn max_items(&self) -> usize {
        self.data.len()
    }
}
