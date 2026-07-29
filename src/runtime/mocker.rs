use std::fmt::Debug;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::block_on;
use crate::runtime::buffer::BlockInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::unbounded;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::BlockNotifier;
use crate::runtime::dev::ItemTag;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::resolve_port_index;
use crate::runtime::wrapped_kernel::WrappedKernel;

/// Native test harness for running one block without a [`Runtime`](crate::runtime::Runtime).
///
/// `Mocker` wraps a kernel in the same block wrapper used by the runtime, but
/// drives `init`, `work`, message handlers, and `deinit` directly. It is useful
/// for focused unit tests and microbenchmarks where constructing a full
/// [`Flowgraph`](crate::runtime::Flowgraph) would add noise.
pub struct Mocker<K: KernelInterface> {
    /// Wrapped Block
    block: WrappedKernel<K>,
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

impl<K: KernelInterface + Kernel + 'static> Mocker<K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id
    }

    /// Get mutable access to the wrapped kernel state used by `Kernel::work`.
    pub fn parts_mut(&mut self) -> (&mut K, &mut MessageOutputs, &mut BlockMeta) {
        let WrappedKernel {
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
        let mut block = WrappedKernel::new(kernel, BlockId(0));
        let mut messages = Vec::new();
        let mut message_sinks: Vec<Receiver<BlockMessage>> = Vec::new();

        for n in <K as KernelInterface>::message_outputs() {
            messages.push(Vec::new());
            let (tx, rx) = unbounded();
            message_sinks.push(rx);
            block
                .mo
                .connect(
                    &PortId::new(*n),
                    BlockInbox::new(tx, BlockNotifier::new()).into(),
                    &PortId::index(0),
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
        let port_id = resolve_port_index(&id, K::message_inputs())
            .ok_or(Error::InvalidMessagePort(self.block.id, id))?;
        let mut io = WorkIo {
            call_again: false,
            finished: false,
        };

        let block_id = self.block.id;
        let WrappedKernel {
            meta, mo, kernel, ..
        } = &mut self.block;
        block_on(kernel.call_handler(block_id, &mut io, mo, meta, port_id, p))
    }

    /// Run the block's `work()` loop synchronously.
    ///
    /// The loop repeats while the block sets [`WorkIo::call_again`]. Message
    /// outputs produced during each call are captured before the next iteration.
    pub fn run(&mut self) {
        block_on(self.run_async());
    }

    /// Run the block's `init()` method synchronously.
    pub fn init(&mut self) {
        block_on(async {
            self.block
                .kernel
                .init(&mut self.block.mo, &self.block.meta)
                .await
                .unwrap();
        });
    }

    /// Run the block's `deinit()` method synchronously.
    pub fn deinit(&mut self) {
        block_on(async {
            self.block
                .kernel
                .deinit(&mut self.block.mo, &self.block.meta)
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
        self.messages.iter_mut().map(std::mem::take).collect()
    }

    /// Run the block's `work()` loop asynchronously.
    ///
    /// Like [`Mocker::run`], this repeats while [`WorkIo::call_again`] is set.
    pub async fn run_async(&mut self) {
        let mut io = WorkIo {
            call_again: false,
            finished: false,
        };

        loop {
            self.block
                .kernel
                .work(&mut io, &mut self.block.mo, &self.block.meta)
                .await
                .unwrap();

            for (n, r) in self.message_sinks.iter_mut().enumerate() {
                while let Ok(m) = r.try_recv() {
                    match m {
                        BlockMessage::Post { data, .. } => {
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
    port_id: PortIndex,
    requirements: BufferRequirements,
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
            port_id: PortIndex::new(0),
            requirements: BufferRequirements::new(),
        }
    }
}

impl<T: Debug + Send + 'static> BufferReader for Reader<T> {
    type Inbox = BlockInbox;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.requirements
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.requirements.merge(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, _inbox: BlockInbox) {
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
    fn port_id(&self) -> PortIndex {
        self.port_id
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
    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        (self.data.as_slice(), &self.tags)
    }
    fn consume(&mut self, n: usize) {
        self.data = self.data.split_off(n);
        self.tags.retain(|x| x.index >= n);

        for t in self.tags.iter_mut() {
            t.index -= n;
        }
    }

    fn max_contiguous_items(&self) -> usize {
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
    port_id: PortIndex,
    requirements: BufferRequirements,
}

impl<T: Clone + Debug + Send + 'static> Default for Writer<T> {
    fn default() -> Self {
        Self {
            data: vec![],
            tags: vec![],
            produced: 0,
            block_id: BlockId(0),
            port_id: PortIndex::new(0),
            requirements: BufferRequirements::new(),
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
    type Inbox = BlockInbox;
    type Reader = Reader<T>;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.requirements
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.requirements.merge(requirements);
    }

    fn init(&mut self, block_id: BlockId, port_id: PortIndex, _inbox: BlockInbox) {
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

    fn port_id(&self) -> PortIndex {
        self.port_id
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
}
