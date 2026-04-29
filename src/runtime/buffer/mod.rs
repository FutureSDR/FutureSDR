//! Buffer Implementations for CPU and Accelerator Memory

// ==================== BURN =======================
#[cfg(feature = "burn")]
pub mod burn;

/// In-place circuit buffer
pub mod circuit;

/// Double-mapped circular buffer
#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
/// Slab buffer
pub mod slab;

// ==================== VULKAN =======================
#[cfg(feature = "vulkan")]
pub mod vulkan;

// ==================== WGPU =======================
#[cfg(feature = "wgpu")]
pub mod wgpu;

// -==================== ZYNQ ========================
#[cfg(all(feature = "zynq", target_os = "linux"))]
pub mod zynq;

use std::any::Any;
use std::future::Future;

use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::BlockNotifier;
use crate::runtime::dev::ItemTag;
use crate::runtime::dev::MaybeSend;
use crate::runtime::dev::Tag;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortId;

/// Shared port configuration collected before the port is connected.
#[derive(Debug, Clone, Copy, Default)]
pub struct PortConfig {
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl PortConfig {
    /// Create empty port configuration.
    pub const fn new() -> Self {
        Self {
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }

    /// Create port configuration with an initial `min_items`.
    pub const fn with_min_items(min_items: usize) -> Self {
        Self {
            min_items: Some(min_items),
            min_buffer_size_in_items: None,
        }
    }

    /// Minimum number of items requested by the port.
    pub const fn min_items(&self) -> Option<usize> {
        self.min_items
    }

    /// Configure the minimum number of items required by the port.
    pub fn set_min_items(&mut self, min_items: usize) {
        self.min_items = Some(min_items);
    }

    /// Raise the minimum number of items to at least `min_items`.
    pub fn set_min_items_max(&mut self, min_items: usize) {
        self.min_items = Some(self.min_items.unwrap_or(0).max(min_items));
    }

    /// Minimum configured buffer size in items.
    pub const fn min_buffer_size_in_items(&self) -> Option<usize> {
        self.min_buffer_size_in_items
    }

    /// Configure the minimum buffer size in items.
    pub fn set_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.min_buffer_size_in_items = Some(min_items);
    }

    /// Raise the minimum buffer size to at least `min_items`.
    pub fn set_min_buffer_size_in_items_max(&mut self, min_items: usize) {
        self.min_buffer_size_in_items =
            Some(self.min_buffer_size_in_items.unwrap_or(0).max(min_items));
    }
}

/// Binding state shared by all ports.
#[derive(Debug, Clone)]
pub enum PortBinding {
    /// Port is only constructed and not yet attached to a concrete block/port id.
    Unbound,
    /// Port is attached to a concrete block/port id inside a flowgraph.
    Bound {
        /// Owning block of the bound port.
        block_id: BlockId,
        /// Port id inside the owning block.
        port_id: PortId,
        /// Inbox used to notify the owning block.
        inbox: BlockInbox,
    },
}

/// Shared per-port state that is independent from the concrete buffer backend.
#[derive(Debug, Clone)]
pub struct PortCore {
    binding: PortBinding,
    config: PortConfig,
}

impl PortCore {
    /// Create an unbound port with empty configuration.
    pub const fn new_disconnected() -> Self {
        Self::with_config(PortConfig::new())
    }

    /// Create an unbound port with the provided configuration.
    pub const fn with_config(config: PortConfig) -> Self {
        Self {
            binding: PortBinding::Unbound,
            config,
        }
    }

    /// Bind the port to the given block/port id and inbox.
    pub fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox) {
        self.binding = PortBinding::Bound {
            block_id,
            port_id,
            inbox,
        };
    }

    /// Whether the port has been bound to a block inside a flowgraph.
    pub fn is_bound(&self) -> bool {
        matches!(self.binding, PortBinding::Bound { .. })
    }

    /// The current binding state.
    pub fn binding(&self) -> &PortBinding {
        &self.binding
    }

    /// Get the bound block id.
    pub fn block_id(&self) -> BlockId {
        match &self.binding {
            PortBinding::Bound { block_id, .. } => *block_id,
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the bound block id if available.
    pub fn block_id_if_bound(&self) -> Option<BlockId> {
        match &self.binding {
            PortBinding::Bound { block_id, .. } => Some(*block_id),
            PortBinding::Unbound => None,
        }
    }

    /// Get the bound port id.
    pub fn port_id(&self) -> PortId {
        match &self.binding {
            PortBinding::Bound { port_id, .. } => port_id.clone(),
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the bound port id if available.
    pub fn port_id_if_bound(&self) -> Option<&PortId> {
        match &self.binding {
            PortBinding::Bound { port_id, .. } => Some(port_id),
            PortBinding::Unbound => None,
        }
    }

    /// Get the bound inbox.
    pub fn inbox(&self) -> BlockInbox {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => inbox.clone(),
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the notifier associated with the bound inbox.
    pub fn notifier(&self) -> BlockNotifier {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => inbox.notifier(),
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Minimum number of items requested by the port.
    pub fn min_items(&self) -> Option<usize> {
        self.config.min_items()
    }

    /// Configure the minimum number of items required by the port.
    pub fn set_min_items(&mut self, min_items: usize) {
        self.config.set_min_items(min_items);
    }

    /// Raise the minimum number of items required by the port.
    pub fn set_min_items_max(&mut self, min_items: usize) {
        self.config.set_min_items_max(min_items);
    }

    /// Minimum configured buffer size in items.
    pub fn min_buffer_size_in_items(&self) -> Option<usize> {
        self.config.min_buffer_size_in_items()
    }

    /// Configure the minimum buffer size in items.
    pub fn set_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.config.set_min_buffer_size_in_items(min_items);
    }

    /// Raise the minimum buffer size in items.
    pub fn set_min_buffer_size_in_items_max(&mut self, min_items: usize) {
        self.config.set_min_buffer_size_in_items_max(min_items);
    }

    /// Create a validation error for an unconnected port.
    pub fn not_connected_error(&self) -> Error {
        match &self.binding {
            PortBinding::Bound {
                block_id, port_id, ..
            } => Error::ValidationError(format!("{block_id:?}:{port_id:?} not connected")),
            PortBinding::Unbound => {
                Error::ValidationError("stream port is not bound to a flowgraph".to_string())
            }
        }
    }
}

/// A peer endpoint captured during connection setup.
#[derive(Debug, Clone)]
pub struct PortEndpoint {
    inbox: BlockInbox,
    port_id: PortId,
}

impl PortEndpoint {
    /// Create a new peer endpoint.
    pub fn new(inbox: BlockInbox, port_id: PortId) -> Self {
        Self { inbox, port_id }
    }

    /// Get the peer inbox.
    pub fn inbox(&self) -> BlockInbox {
        self.inbox.clone()
    }

    /// Get the peer port id.
    pub fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

/// Circuit-return path back to the start of an in-place circuit.
#[derive(Debug, Clone)]
pub(crate) struct CircuitReturn<Q> {
    notifier: BlockNotifier,
    queue: Q,
}

impl<Q> CircuitReturn<Q> {
    /// Create a new circuit-return path.
    pub(crate) fn new(notifier: BlockNotifier, queue: Q) -> Self {
        Self { notifier, queue }
    }

    /// Notify the circuit start that a buffer was returned or consumed.
    pub(crate) fn notify(&self) {
        self.notifier.notify();
    }

    /// Access the queue used to return buffers to the circuit start.
    pub(crate) fn queue(&self) -> &Q {
        &self.queue
    }
}

/// A backend state that is either disconnected or fully connected.
#[derive(Debug)]
pub enum ConnectionState<T> {
    /// No backend has been connected yet.
    Disconnected,
    /// The backend is fully connected and ready to use.
    Connected(T),
}

impl<T> ConnectionState<T> {
    /// Create a disconnected backend state.
    pub const fn disconnected() -> Self {
        Self::Disconnected
    }

    /// Whether the backend has been connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    /// Borrow the connected backend if present.
    pub fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Disconnected => None,
            Self::Connected(value) => Some(value),
        }
    }

    /// Borrow the connected backend mutably if present.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Disconnected => None,
            Self::Connected(value) => Some(value),
        }
    }

    /// Get the connected backend, panicking if it is still disconnected.
    pub fn connected(&self) -> &T {
        self.as_ref()
            .expect("buffer backend is disconnected after validation")
    }

    /// Get the connected backend mutably, panicking if it is still disconnected.
    pub fn connected_mut(&mut self) -> &mut T {
        self.as_mut()
            .expect("buffer backend is disconnected after validation")
    }

    /// Replace the state with a connected backend.
    pub fn set_connected(&mut self, value: T) {
        *self = Self::Connected(value);
    }

    /// Take the connected backend out of the state.
    pub fn take_connected(&mut self) -> Option<T> {
        match std::mem::replace(self, Self::Disconnected) {
            Self::Disconnected => None,
            Self::Connected(value) => Some(value),
        }
    }
}

/// Type-erased reader side of a stream buffer.
///
/// This is the core runtime trait every buffer reader implements. Custom block
/// authors normally use higher-level traits such as [`CpuBufferReader`] or
/// [`InplaceReader`] instead of calling these methods directly.
#[async_trait]
pub trait BufferReader: Any {
    /// Return this reader as [`Any`] for runtime downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Initialize the reader with its owning block, port id, and inbox.
    ///
    /// This sets the own block ID, Port ID, and message receiver so that it can
    /// be communicated to the other end when making connections.
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox);
    /// Validate that this reader is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Notify upstream writers that this reader is done.
    async fn notify_finished(&mut self);
    /// Mark this reader because the upstream writer is done.
    ///
    /// The Block will usually process the remaining samples and shut down.
    fn finish(&mut self);
    /// Return whether the upstream writer has marked this buffer as done.
    fn finished(&self) -> bool;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port id.
    fn port_id(&self) -> PortId;
}

/// Type-erased writer side of a stream buffer.
///
/// This is the core runtime trait every buffer writer implements. Custom block
/// authors normally use higher-level traits such as [`CpuBufferWriter`] or
/// [`InplaceWriter`] instead of calling these methods directly.
pub trait BufferWriter {
    /// The corresponding reader.
    type Reader: BufferReader;
    /// Initialize the writer with its owning block, port id, and inbox.
    ///
    /// This sets the own block ID, Port ID, and message receiver so that it can
    /// be communicated to the other end when making connections.
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: BlockInbox);
    /// Validate that this writer is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Connect the writer to a matching reader.
    fn connect(&mut self, dest: &mut Self::Reader);
    /// Connect the writer to a type-erased reader.
    fn connect_dyn(&mut self, dest: &mut dyn BufferReader) -> Result<(), Error> {
        if let Some(concrete) = dest.as_any_mut().downcast_mut::<Self::Reader>() {
            self.connect(concrete);
            Ok(())
        } else {
            Err(Error::ValidationError(
                "dyn BufferReader has wrong type".to_string(),
            ))
        }
    }
    /// Notify downstream blocks that we are done.
    fn notify_finished(&mut self) -> impl Future<Output = ()> + MaybeSend;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port id.
    fn port_id(&self) -> PortId;
}

/// A buffer writer that can close an in-place circuit to a matching end.
///
/// Circuit-capable buffers are still connected with the normal
/// [`BufferWriter::connect`] stream connection. Closing the circuit is the
/// additional step that wires the downstream end back to the upstream start so
/// buffers can circulate.
pub trait CircuitWriter: BufferWriter {
    /// The circuit end type accepted by this writer.
    type CircuitEnd;

    /// Close the circuit to the given end.
    fn close_circuit(&mut self, dst: &mut Self::CircuitEnd);
}

/// Trait alias-style marker for sample types supported by CPU buffers.
pub trait CpuSample: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

impl<T> CpuSample for T where T: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

/// Reader API for out-of-place CPU stream buffers.
///
/// Blocks use this trait in `work()` to inspect available input samples and
/// then call [`consume`](Self::consume) for the number of items processed.
pub trait CpuBufferReader: BufferReader + Default + MaybeSend {
    /// Item type carried by this stream input.
    type Item: CpuSample;
    /// Get available samples.
    fn slice(&mut self) -> &[Self::Item] {
        self.slice_with_tags().0
    }
    /// Get available samples and tags.
    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>);
    /// Consume items from the input buffer.
    fn consume(&mut self, n: usize);
    /// Configure the minimum number of items required in
    /// [work()](crate::runtime::dev::Kernel::work)
    ///
    /// This defines the minimum number of samples that the block needs to proceed. For example, an
    /// FFT block requires samples corresponding to the FFT size.
    fn set_min_items(&mut self, n: usize);
    /// Configure the minimum buffer size
    ///
    /// This sets the minimum number of samples that the buffer can take. This is independent from
    /// any requirements in [work()](crate::runtime::dev::Kernel::work) but mainly for performance reasons, i.e., it
    /// defines the tradeoff between throughput and latency.
    ///
    /// By default, it will be set to the value defined in
    /// [`crate::runtime::config::Config`].
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Return the maximum number of items that fit in the buffer.
    fn max_items(&self) -> usize;
}

/// Writer API for out-of-place CPU stream buffers.
///
/// Blocks use this trait in `work()` to get writable output space and then call
/// [`produce`](Self::produce) for the number of initialized items.
pub trait CpuBufferWriter: BufferWriter + Default + MaybeSend {
    /// Item type carried by this stream output.
    type Item: CpuSample;
    /// Get available output buffer space.
    fn slice(&mut self) -> &mut [Self::Item] {
        self.slice_with_tags().0
    }
    /// Available buffer space and tags.
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>);
    /// Produce initialized items into the output buffer.
    fn produce(&mut self, n: usize);
    /// Configure the minimum number of items required in
    /// [work()](crate::runtime::dev::Kernel::work)
    ///
    /// This defines the minimum number of samples that the block needs to proceed. For example, an
    /// FFT block requires samples corresponding to the FFT size.
    fn set_min_items(&mut self, n: usize);
    /// Configure the minimum buffer size
    ///
    /// This sets the minimum number of samples that the buffer can take. This is independent from
    /// any requirements in [work()](crate::runtime::dev::Kernel::work) but mainly for performance reasons, i.e., it
    /// defines the tradeoff between throughput and latency.
    ///
    /// By default, it will be set to the value defined in
    /// [`crate::runtime::config::Config`].
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Return the maximum number of items that fit in the buffer.
    fn max_items(&self) -> usize;
}

/// Owned buffer chunk passed through an in-place stream circuit.
pub trait InplaceBuffer {
    /// Type of the samples in the buffer.
    type Item: CpuSample;
    /// Set the number of valid samples in the buffer.
    fn set_valid(&mut self, valid: usize);
    /// Access the buffer samples.
    fn slice(&mut self) -> &mut [Self::Item];
    /// Access the buffer samples and tags.
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>);
}

/// Reader half of an in-place circuit buffer.
pub trait InplaceReader: BufferReader + Default + MaybeSend {
    /// Item type carried by this in-place reader.
    type Item: CpuSample;
    /// Buffer chunk type moved through this reader.
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Get the next full buffer, if one is available.
    fn get_full_buffer(&mut self) -> Option<Self::Buffer>;
    /// Return whether more full buffers are immediately available.
    fn has_more_buffers(&mut self) -> bool;
    /// Return an empty buffer to the beginning of the circuit.
    fn put_empty_buffer(&mut self, buffer: Self::Buffer);
    /// Notify the circuit start that we consumed a buffer.
    fn notify_consumed_buffer(&mut self);
}

/// Writer half of an in-place circuit buffer.
pub trait InplaceWriter: BufferWriter + Default + MaybeSend {
    /// Item type carried by this in-place writer.
    type Item: CpuSample;
    /// Buffer chunk type moved through this writer.
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Submit a full buffer to the downstream reader.
    fn put_full_buffer(&mut self, buffer: Self::Buffer);

    /// Get an empty buffer, if one is available.
    ///
    /// This is typically used in sources, i.e., when there is no inplace reader
    fn get_empty_buffer(&mut self) -> Option<Self::Buffer>;
    /// Return whether more empty buffers are immediately available.
    fn has_more_buffers(&mut self) -> bool;
    /// Inject new empty buffers using the configured default item capacity.
    fn inject_buffers(&mut self, n_buffers: usize) {
        let n_items =
            futuresdr::runtime::config::config().buffer_size / std::mem::size_of::<Self::Item>();
        self.inject_buffers_with_items(n_buffers, n_items);
    }
    /// Inject new empty buffers with an explicit item capacity.
    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize);
}

#[cfg(not(target_arch = "wasm32"))]
/// Default [`CpuBufferReader`] implementation.
pub type DefaultCpuReader<D> = circular::Reader<D>;
/// Default [`CpuBufferWriter`] implementation.
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultCpuWriter<D> = circular::Writer<D>;
#[cfg(target_arch = "wasm32")]
/// Default [`CpuBufferReader`] implementation.
pub type DefaultCpuReader<D> = slab::Reader<D>;
#[cfg(target_arch = "wasm32")]
/// Default [`CpuBufferWriter`] implementation.
pub type DefaultCpuWriter<D> = slab::Writer<D>;

/// Helper for adding tags to an output buffer.
pub struct Tags<'a> {
    tags: &'a mut Vec<ItemTag>,
    offset: usize,
}

impl<'a> Tags<'a> {
    /// Create an output tag helper.
    ///
    /// Should only be constructed in buffer implementations.
    pub fn new(tags: &'a mut Vec<ItemTag>, offset: usize) -> Self {
        Self { tags, offset }
    }
    /// Add a tag at an index relative to the current output slice.
    pub fn add_tag(&mut self, index: usize, tag: Tag) {
        self.tags.push(ItemTag {
            index: index + self.offset,
            tag,
        });
    }
}
