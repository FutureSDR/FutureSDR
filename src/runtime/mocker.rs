use std::any::Any;
use std::fmt::Debug;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::ThreadSafeMode;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::channel;
use crate::runtime::config::config;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::BlockNotifier;
use crate::runtime::dev::ItemTag;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::wrapped_kernel::NormalWrappedKernel;

/// Native test harness for running one block without a [`Runtime`](crate::runtime::Runtime).
///
/// `Mocker` wraps a kernel in the same block wrapper used by the runtime, but
/// drives `init`, `work`, message handlers, and `deinit` directly. It is useful
/// for focused unit tests and microbenchmarks where constructing a full
/// [`Flowgraph`](crate::runtime::Flowgraph) would add noise.
pub struct Mocker<K: KernelInterface> {
    /// Wrapped Block
    block: NormalWrappedKernel<K>,
    message_sinks: Vec<Receiver<BlockMessage>>,
    messages: Vec<Vec<Pmt>>,
}

impl<K: KernelInterface + 'static> Deref for Mocker<K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.block.kernel
    }
}
impl<K: KernelInterface + 'static> DerefMut for Mocker<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.block.kernel
    }
}

impl<K: KernelInterface + crate::runtime::dev::Kernel + 'static> Mocker<K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id
    }

    /// Get mutable access to the wrapped kernel state used by `Kernel::work`.
    pub fn parts_mut(&mut self) -> (&mut K, &mut MessageOutputs, &mut BlockMeta) {
        let NormalWrappedKernel {
            kernel, mo, meta, ..
        } = &mut self.block;
        (kernel, mo, meta)
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        &self.block.meta
    }

    /// Get mutable block metadata.
    pub fn meta_mut(&mut self) -> &mut BlockMeta {
        &mut self.block.meta
    }

    /// Create a mocker around one kernel instance.
    ///
    /// Message output ports declared by the block are connected to internal
    /// sinks so tests can inspect emitted PMTs with [`Mocker::messages`] or
    /// [`Mocker::take_messages`].
    pub fn new(kernel: K) -> Self {
        let mut block = NormalWrappedKernel::new(kernel, BlockId(0));
        let mut messages = Vec::new();
        let mut message_sinks: Vec<Receiver<BlockMessage>> = Vec::new();
        let msg_len = config().queue_size;

        for n in <K as KernelInterface>::message_outputs() {
            messages.push(Vec::new());
            let (tx, rx) = channel(msg_len);
            message_sinks.push(rx);
            block
                .mo
                .connect(
                    &PortId::new(*n),
                    BlockInbox::new(tx, BlockNotifier::new()),
                    &PortId::new("input"),
                )
                .unwrap();
        }

        Mocker {
            block,
            message_sinks,
            messages,
        }
    }

    /// Call one message handler of the block and return its PMT result.
    ///
    /// This executes the generated handler dispatch directly. It does not run
    /// `work()` afterward; call [`Mocker::run`] if the handler only queued state
    /// that should be processed by the work loop.
    pub fn post(&mut self, id: impl Into<PortId>, p: Pmt) -> Result<Pmt, Error> {
        let id = id.into();
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: false,
        };

        let NormalWrappedKernel {
            meta, mo, kernel, ..
        } = &mut self.block;
        crate::runtime::block_on(kernel.call_handler(&mut io, mo, meta, id, p))
            .map_err(|e| Error::HandlerError(e.to_string()))
    }

    /// Run the block's `work()` loop synchronously.
    ///
    /// The loop repeats while the block sets [`WorkIo::call_again`]. Message
    /// outputs produced during each call are captured before the next iteration.
    pub fn run(&mut self) {
        crate::runtime::block_on(self.run_async());
    }

    /// Run the block's `init()` method synchronously.
    pub fn init(&mut self) {
        crate::runtime::block_on(async {
            self.block
                .kernel
                .init(&mut self.block.mo, &mut self.block.meta)
                .await
                .unwrap();
        });
    }

    /// Run the block's `deinit()` method synchronously.
    pub fn deinit(&mut self) {
        crate::runtime::block_on(async {
            self.block
                .kernel
                .deinit(&mut self.block.mo, &mut self.block.meta)
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

    /// Run the block's `work()` loop asynchronously.
    ///
    /// Like [`Mocker::run`], this repeats while [`WorkIo::call_again`] is set.
    pub async fn run_async(&mut self) {
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: false,
        };

        loop {
            self.block
                .kernel
                .work(&mut io, &mut self.block.mo, &mut self.block.meta)
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
/// Mock CPU input buffer for [`Mocker`].
///
/// Use [`Reader::set`] or [`Reader::set_with_tags`] before running the block.
/// Consumed items are removed from the front of the buffer, matching the normal
/// [`CpuBufferReader`] contract.
pub struct Reader<T: Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
    block_id: BlockId,
    port_id: PortId,
}

impl<T: Debug + Send + 'static> Reader<T> {
    /// Replace the readable input items.
    pub fn set(&mut self, data: Vec<T>)
    where
        T: Debug + Send + 'static,
    {
        self.set_with_tags(data, Vec::new());
    }

    /// Replace the readable input items and their tags.
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

impl<T: Debug + Send + 'static> BufferReader for Reader<T> {
    type Mode = ThreadSafeMode;

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn init(&mut self, block_id: BlockId, port_id: PortId, _inbox: BlockInbox) {
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
/// Mock CPU output buffer for [`Mocker`].
///
/// Reserve capacity before running the block. Produced items are appended to the
/// buffer and can be cloned with [`Writer::get`] or drained with
/// [`Writer::take`].
pub struct Writer<T: Clone + Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
    produced: usize,
    block_id: BlockId,
    port_id: PortId,
}

impl<T: Clone + Debug + Send + 'static> Default for Writer<T> {
    fn default() -> Self {
        Self {
            data: vec![],
            tags: vec![],
            produced: 0,
            block_id: BlockId(0),
            port_id: PortId::new("output"),
        }
    }
}

impl<T: Clone + Debug + Send + 'static> Writer<T> {
    /// Reserve writable capacity in the output buffer.
    pub fn reserve(&mut self, n: usize)
    where
        T: Default,
    {
        self.data.clear();
        self.tags.clear();
        self.produced = 0;
        self.data.resize_with(n, T::default);
    }
    /// Clone all produced items and tags without clearing them.
    pub fn get(&self) -> (Vec<T>, Vec<ItemTag>) {
        (self.data[..self.produced].to_vec(), self.tags.clone())
    }
    /// Drain all produced items and tags.
    pub fn take(&mut self) -> (Vec<T>, Vec<ItemTag>) {
        let mut data = std::mem::take(&mut self.data);
        data.truncate(self.produced);
        self.produced = 0;
        (data, std::mem::take(&mut self.tags))
    }
}

impl<T: Clone + Debug + Send + 'static> BufferWriter for Writer<T> {
    type Mode = ThreadSafeMode;
    type Reader = Reader<T>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, _inbox: BlockInbox) {
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
        &mut self.data[self.produced..]
    }
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        (
            &mut self.data[self.produced..],
            Tags::new(&mut self.tags, self.produced),
        )
    }
    fn produce(&mut self, n: usize) {
        assert!(
            self.produced + n <= self.data.len(),
            "mocker writer produced more items than reserved"
        );
        self.produced += n;
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items has no effect in with mocker");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items has no effect in a mocker");
    }

    fn max_items(&self) -> usize {
        self.data.len() - self.produced
    }
}
