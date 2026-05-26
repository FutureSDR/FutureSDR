//! Stream buffer traits and built-in buffer implementations.
//!
//! Buffers are the transport layer between stream ports. A connection pairs one
//! writer with one reader; the concrete buffer implementation decides whether
//! that means CPU slices, in-place buffer chunks, GPU resources, tensors, or
//! hardware-owned DMA memory.
//!
//! Application code normally uses the default buffer types exposed by existing
//! blocks. Custom block and runtime-extension authors use the traits in this
//! module to select a buffer family or implement a new transport.

// ==================== BURN =======================
#[cfg(feature = "burn")]
pub mod burn;

/// In-place circuit buffer.
pub mod circuit;

/// Same-thread CPU buffer for local domains.
pub mod local;
#[doc(hidden)]
pub mod queued;

/// Double-mapped circular CPU buffer.
#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
/// Queue-backed slab CPU buffer.
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
use std::fmt::Debug;
use std::future::Future;

use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::BlockNotifier;
use crate::runtime::dev::ItemTag;
use crate::runtime::dev::LocalBlockInbox;
use crate::runtime::dev::LocalBlockNotifier;
use crate::runtime::dev::Tag;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortId;

/// Shared stream-port configuration collected before a port is connected.
///
/// Buffer implementations use this to remember constraints requested by blocks
/// before the flowgraph has connected concrete peer endpoints.
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
    ///
    /// A scheduler should avoid calling the block until this many items are
    /// available to read or write, unless the peer has finished.
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

/// Wake-only handle stored by stream buffers for hot-path notifications.
pub trait BufferNotifier: Clone + Debug + 'static {
    /// Wake the owning block without sending a message.
    fn notify(&self);
}

impl BufferNotifier for BlockNotifier {
    #[inline(always)]
    fn notify(&self) {
        BlockNotifier::notify(self);
    }
}

impl BufferNotifier for LocalBlockNotifier {
    #[inline(always)]
    fn notify(&self) {
        LocalBlockNotifier::notify(self);
    }
}

/// Wake/message handle stored by stream buffers.
pub trait BufferInbox: Clone + Debug + 'static {
    /// Wake the owning block without sending a message.
    fn notify(&self);
    /// Notify the destination block that one stream input port is done.
    fn stream_input_done(&self, input_id: PortId) -> impl Future<Output = Result<(), Error>>;
    /// Notify the destination block that one stream output port is done.
    fn stream_output_done(&self, output_id: PortId) -> impl Future<Output = Result<(), Error>>;
}

impl BufferInbox for BlockInbox {
    #[inline(always)]
    fn notify(&self) {
        BlockInbox::notify(self);
    }

    async fn stream_input_done(&self, input_id: PortId) -> Result<(), Error> {
        BlockInbox::stream_input_done(self, input_id).await
    }

    async fn stream_output_done(&self, output_id: PortId) -> Result<(), Error> {
        BlockInbox::stream_output_done(self, output_id).await
    }
}

impl BufferInbox for LocalBlockInbox {
    #[inline(always)]
    fn notify(&self) {
        LocalBlockInbox::notify(self);
    }

    async fn stream_input_done(&self, input_id: PortId) -> Result<(), Error> {
        LocalBlockInbox::push(
            self,
            crate::runtime::BlockMessage::StreamInputDone { input_id },
        );
        Ok(())
    }

    async fn stream_output_done(&self, output_id: PortId) -> Result<(), Error> {
        LocalBlockInbox::push(
            self,
            crate::runtime::BlockMessage::StreamOutputDone { output_id },
        );
        Ok(())
    }
}

/// Stream-port inboxes available when a block's ports are initialized.
#[derive(Clone, Debug)]
pub struct PortInboxes {
    thread_safe: BlockInbox,
    local: Option<LocalBlockInbox>,
}

impl PortInboxes {
    /// Create init handles for a normal/send-capable block.
    pub fn thread_safe(thread_safe: BlockInbox) -> Self {
        Self {
            thread_safe,
            local: None,
        }
    }

    /// Create init handles for a local-domain block.
    pub fn local(thread_safe: BlockInbox, local: LocalBlockInbox) -> Self {
        Self {
            thread_safe,
            local: Some(local),
        }
    }

    /// Get the send-capable ingress handle.
    pub fn thread_safe_inbox(&self) -> BlockInbox {
        self.thread_safe.clone()
    }

    /// Get the direct local-domain handle.
    pub fn local_inbox(&self) -> LocalBlockInbox {
        self.local
            .clone()
            .expect("local buffer used outside a local-domain block")
    }
}

/// Selects which wake/message mechanism a buffer stores.
pub trait BufferMode: 'static {
    /// Inbox handle stored by buffers in this mode.
    type Inbox: BufferInbox;
    /// Wake-only handle stored on hot paths.
    type Notifier: BufferNotifier;

    /// Select this mode's inbox from the handles provided during port init.
    fn inbox(inboxes: &PortInboxes) -> Self::Inbox;
    /// Extract a wake-only notifier from this mode's inbox.
    fn notifier(inbox: &Self::Inbox) -> Self::Notifier;
}

/// Send-capable wake mode for normal buffers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadSafeMode;

impl BufferMode for ThreadSafeMode {
    type Inbox = BlockInbox;
    type Notifier = BlockNotifier;

    fn inbox(inboxes: &PortInboxes) -> Self::Inbox {
        inboxes.thread_safe_inbox()
    }

    fn notifier(inbox: &Self::Inbox) -> Self::Notifier {
        inbox.notifier()
    }
}

/// Same-thread local-domain wake mode for local buffers.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalMode;

impl BufferMode for LocalMode {
    type Inbox = LocalBlockInbox;
    type Notifier = LocalBlockNotifier;

    fn inbox(inboxes: &PortInboxes) -> Self::Inbox {
        inboxes.local_inbox()
    }

    fn notifier(inbox: &Self::Inbox) -> Self::Notifier {
        inbox.notifier()
    }
}

/// Binding state shared by all stream ports.
#[derive(Debug, Clone)]
pub enum PortBinding<M: BufferMode = ThreadSafeMode> {
    /// Port is only constructed and not yet attached to a concrete block/port id.
    Unbound,
    /// Port is attached to a concrete block/port id inside a flowgraph.
    Bound {
        /// Owning block of the bound port.
        block_id: BlockId,
        /// Port id inside the owning block.
        port_id: PortId,
        /// Inbox used to notify the owning block.
        inbox: M::Inbox,
    },
}

/// Shared per-port state that is independent from the concrete buffer backend.
#[derive(Debug, Clone)]
pub struct PortCore<M: BufferMode = ThreadSafeMode> {
    binding: PortBinding<M>,
    config: PortConfig,
}

impl<M: BufferMode> PortCore<M> {
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
    pub fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: M::Inbox) {
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
    pub fn binding(&self) -> &PortBinding<M> {
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
    pub fn inbox(&self) -> M::Inbox {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => inbox.clone(),
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the wake-only notifier associated with the bound inbox.
    pub fn notifier(&self) -> M::Notifier {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => M::notifier(inbox),
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
pub struct PortEndpoint<M: BufferMode = ThreadSafeMode> {
    inbox: M::Inbox,
    port_id: PortId,
}

impl<M: BufferMode> PortEndpoint<M> {
    /// Create a new peer endpoint.
    pub fn new(inbox: M::Inbox, port_id: PortId) -> Self {
        Self { inbox, port_id }
    }

    /// Get the peer inbox.
    pub fn inbox(&self) -> M::Inbox {
        self.inbox.clone()
    }

    /// Get the peer port id.
    pub fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

/// Circuit-return path back to the start of an in-place circuit.
#[derive(Debug, Clone)]
pub(crate) struct CircuitReturn<I, Q> {
    inbox: I,
    queue: Q,
}

impl<I: BufferInbox, Q> CircuitReturn<I, Q> {
    /// Create a new circuit-return path.
    pub(crate) fn new(inbox: I, queue: Q) -> Self {
        Self { inbox, queue }
    }

    /// Notify the circuit start that a buffer was returned or consumed.
    pub(crate) fn notify(&self) {
        self.inbox.notify();
    }

    /// Access the queue used to return buffers to the circuit start.
    pub(crate) fn queue(&self) -> &Q {
        &self.queue
    }
}

/// A backend state that is either disconnected or fully connected.
///
/// Buffer implementations use this helper when their reader or writer can be
/// constructed before the peer endpoint exists, then filled in during
/// connection setup.
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
    ///
    /// Call this after `validate()` has proven the buffer is connected.
    pub fn connected(&self) -> &T {
        self.as_ref()
            .expect("buffer backend is disconnected after validation")
    }

    /// Get the connected backend mutably, panicking if it is still disconnected.
    ///
    /// Call this after `validate()` has proven the buffer is connected.
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

/// Send-capable reader marker for stream buffers.
///
/// Native normal flowgraphs use this to ensure the reader type and its
/// finish-notification future can cross worker threads. The reader methods come
/// from [`BufferReader`].
pub trait SendBufferReader: BufferReader<notify_finished(..): Send> + Send {}

/// Send-capable writer marker for stream buffers.
///
/// Native normal flowgraphs use this to ensure the writer type, its matching
/// reader, and its finish-notification future can cross worker threads. The
/// writer methods come from [`BufferWriter`].
pub trait SendBufferWriter:
    BufferWriter<Reader: SendBufferReader, notify_finished(..): Send> + Send
{
}

/// Type-erased reader side of a stream buffer.
pub trait AnyBufferReader: Any {
    /// Return this reader as [`Any`] for runtime downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Initialize the reader from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_id: PortId, inboxes: &PortInboxes);
    /// Validate that this reader is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Mark this reader because the upstream writer is done.
    fn finish(&mut self);
    /// Return whether the upstream writer has marked this buffer as done.
    fn finished(&self) -> bool;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port id.
    fn port_id(&self) -> PortId;
}

impl<T: BufferReader> AnyBufferReader for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        BufferReader::as_any_mut(self)
    }

    fn init_from(&mut self, block_id: BlockId, port_id: PortId, inboxes: &PortInboxes) {
        BufferReader::init_from(self, block_id, port_id, inboxes);
    }

    fn validate(&self) -> Result<(), Error> {
        BufferReader::validate(self)
    }

    fn finish(&mut self) {
        BufferReader::finish(self);
    }

    fn finished(&self) -> bool {
        BufferReader::finished(self)
    }

    fn block_id(&self) -> BlockId {
        BufferReader::block_id(self)
    }

    fn port_id(&self) -> PortId {
        BufferReader::port_id(self)
    }
}

/// Reader side of a stream buffer.
///
/// Native send-capable readers are derived from this trait through a blanket
/// impl when the type and returned futures permit it.
pub trait BufferReader: Any {
    /// Wake/message mode used by this buffer.
    type Mode: BufferMode;
    /// Return this reader as [`Any`] for runtime downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Initialize the reader with its owning block, port id, and inbox.
    fn init(
        &mut self,
        block_id: BlockId,
        port_id: PortId,
        inbox: <Self::Mode as BufferMode>::Inbox,
    );
    /// Initialize the reader from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_id: PortId, inboxes: &PortInboxes) {
        self.init(block_id, port_id, Self::Mode::inbox(inboxes));
    }
    /// Replace the inbox/wake handle for this already-bound reader.
    fn set_inbox(&mut self, inbox: <Self::Mode as BufferMode>::Inbox) {
        let block_id = self.block_id();
        let port_id = self.port_id();
        self.init(block_id, port_id, inbox);
    }
    /// Validate that this reader is connected and ready to run.
    ///
    /// The runtime calls this during flowgraph startup before any block `init()`
    /// method runs.
    fn validate(&self) -> Result<(), Error>;
    /// Notify upstream writers that this reader is done.
    ///
    /// Implementations usually forward this signal through the peer
    /// [`BlockInbox`] so the upstream block can stop producing to this port.
    fn notify_finished(&mut self) -> impl Future<Output = ()>
    where
        Self: Sized;
    /// Mark this reader because the upstream writer is done.
    fn finish(&mut self);
    /// Return whether the upstream writer has marked this buffer as done.
    fn finished(&self) -> bool;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port id.
    fn port_id(&self) -> PortId;
}

impl<T> SendBufferReader for T where T: BufferReader<notify_finished(..): Send> + Send + 'static {}

/// Writer side of a stream buffer.
///
/// Native send-capable writers are derived from this trait through a blanket
/// impl when the type, reader, and returned futures permit it.
pub trait BufferWriter {
    /// Wake/message mode used by this buffer.
    type Mode: BufferMode;
    /// The corresponding local reader.
    type Reader: BufferReader<Mode = Self::Mode>;
    /// Initialize the writer with its owning block, port id, and inbox.
    fn init(
        &mut self,
        block_id: BlockId,
        port_id: PortId,
        inbox: <Self::Mode as BufferMode>::Inbox,
    );
    /// Initialize the writer from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_id: PortId, inboxes: &PortInboxes) {
        self.init(block_id, port_id, Self::Mode::inbox(inboxes));
    }
    /// Replace the inbox/wake handle for this already-bound writer.
    fn set_inbox(&mut self, inbox: <Self::Mode as BufferMode>::Inbox) {
        let block_id = self.block_id();
        let port_id = self.port_id();
        self.init(block_id, port_id, inbox);
    }
    /// Validate that this writer is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Connect the writer to a matching reader.
    ///
    /// This is called while the flowgraph is being constructed, before runtime
    /// startup. Implementations should store peer queues, not transfer samples.
    fn connect(&mut self, dest: &mut Self::Reader);
    /// Connect the writer to a type-erased local reader.
    fn connect_dyn(&mut self, dest: &mut dyn AnyBufferReader) -> Result<(), Error> {
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
    ///
    /// Implementations usually mark the peer reader as finished and wake the
    /// downstream block.
    fn notify_finished(&mut self) -> impl Future<Output = ()>;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port id.
    fn port_id(&self) -> PortId;
}

impl<T> SendBufferWriter for T
where
    T: BufferWriter<notify_finished(..): Send> + Send,
    T::Reader: SendBufferReader,
{
}

/// Buffer writer that can close an in-place circuit to a matching end.
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

/// Send-capable circuit writer marker.
pub trait SendCircuitWriter: CircuitWriter + SendBufferWriter {}

impl<T> SendCircuitWriter for T where T: CircuitWriter + SendBufferWriter {}

/// Trait alias-style marker for sample types supported by CPU buffers.
pub trait CpuSample: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

impl<T> CpuSample for T where T: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

/// Send-capable reader marker for out-of-place CPU stream buffers.
///
/// The CPU methods come from [`CpuBufferReader`].
pub trait SendCpuBufferReader: CpuBufferReader + SendBufferReader {}

/// CPU stream reader API.
///
/// This is the primary local API. Native send-capable readers are derived from
/// this trait through a blanket impl.
pub trait CpuBufferReader: BufferReader + Default {
    /// Item type.
    type Item: CpuSample;
    /// Get readable slice and associated tags.
    ///
    /// The returned slice starts at the next unread item. Tags use indices
    /// relative to this slice.
    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>);
    /// Get readable slice.
    fn slice(&mut self) -> &[Self::Item] {
        self.slice_with_tags().0
    }
    /// Mark `n` items as consumed.
    ///
    /// `n` must not exceed the length of the last readable slice the block
    /// decided to consume.
    fn consume(&mut self, n: usize);
    /// Set minimum number of readable items.
    fn set_min_items(&mut self, n: usize);
    /// Set minimum buffer size.
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Return the maximum number of items that fit in the buffer.
    fn max_items(&self) -> usize;
}

impl<T> SendCpuBufferReader for T where T: CpuBufferReader + SendBufferReader {}

/// Send-capable writer marker for out-of-place CPU stream buffers.
///
/// The CPU methods come from [`CpuBufferWriter`].
pub trait SendCpuBufferWriter: CpuBufferWriter + SendBufferWriter {}

/// CPU stream writer API.
///
/// This is the primary local API. Native send-capable writers are derived from
/// this trait through a blanket impl.
pub trait CpuBufferWriter: BufferWriter + Default {
    /// Item type.
    type Item: CpuSample;
    /// Get writable slice and tag sink.
    ///
    /// The returned slice starts at the next unproduced output item. Tags added
    /// through [`Tags`] use indices relative to this slice.
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>);
    /// Get writable slice.
    fn slice(&mut self) -> &mut [Self::Item] {
        self.slice_with_tags().0
    }
    /// Mark `n` items as produced.
    ///
    /// `n` must not exceed the number of items written into the last writable
    /// slice.
    fn produce(&mut self, n: usize);
    /// Set minimum number of writable items.
    fn set_min_items(&mut self, n: usize);
    /// Set minimum buffer size.
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Maximum writable items.
    fn max_items(&self) -> usize;
}

impl<T> SendCpuBufferWriter for T where T: CpuBufferWriter + SendBufferWriter {}

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
pub trait InplaceReader: BufferReader + Default {
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
pub trait InplaceWriter: BufferWriter + Default {
    /// Item type carried by this in-place writer.
    type Item: CpuSample;
    /// Buffer chunk type moved through this writer.
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Submit a full buffer to the downstream reader.
    fn put_full_buffer(&mut self, buffer: Self::Buffer);

    /// Get an empty buffer, if one is available.
    ///
    /// This is typically used by source-style blocks that create data at the
    /// beginning of an in-place circuit.
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

/// Send-capable in-place reader marker.
pub trait SendInplaceReader: InplaceReader + SendBufferReader {}

impl<T> SendInplaceReader for T where T: InplaceReader + SendBufferReader {}

/// Send-capable in-place writer marker.
pub trait SendInplaceWriter: InplaceWriter + SendBufferWriter {}

impl<T> SendInplaceWriter for T where T: InplaceWriter + SendBufferWriter {}

#[cfg(not(target_arch = "wasm32"))]
/// Default send-capable [`CpuBufferReader`] implementation.
pub type DefaultCpuReader<D> = circular::Reader<D>;
/// Default send-capable [`CpuBufferWriter`] implementation.
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultCpuWriter<D> = circular::Writer<D>;
#[cfg(target_arch = "wasm32")]
/// Default [`CpuBufferReader`] implementation on WASM.
pub type DefaultCpuReader<D> = slab::Reader<D>;
#[cfg(target_arch = "wasm32")]
/// Default [`CpuBufferWriter`] implementation on WASM.
pub type DefaultCpuWriter<D> = slab::Writer<D>;
/// Local [`CpuBufferReader`] implementation.
pub type LocalCpuReader<D> = local::Reader<D>;
/// Local [`CpuBufferWriter`] implementation.
pub type LocalCpuWriter<D> = local::Writer<D>;

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
