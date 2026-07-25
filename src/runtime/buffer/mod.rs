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
//!
//! # Stream termination and fanout
//!
//! When a block finishes, each of its input readers notifies the upstream block
//! that the corresponding writer is no longer needed. The upstream block then
//! finishes as a whole and notifies every reader connected to all of its output
//! writers. For a writer with multiple readers, one reader finishing therefore
//! stops further production for every reader. The other readers may still
//! process items already present in their buffers.

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

// ==================== WGPU =======================
/// WGPU accelerator handoff buffers.
#[cfg(feature = "wgpu")]
pub mod wgpu;

use std::any::Any;
use std::any::TypeId;
use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortIndex;
pub use crate::runtime::block_inbox::BlockInbox;
pub use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::config::config;
use crate::runtime::dev::BlockNotifier;
use crate::runtime::dev::ItemTag;
use crate::runtime::dev::LocalBlockNotifier;
use crate::runtime::dev::Tag;

/// Stream-buffer constraints configured on a port and exchanged during connection setup.
///
/// Blocks configure these constraints through buffer-specific APIs such as
/// [`CpuBufferReader::set_min_items`] and [`CpuBufferWriter::set_min_items`].
/// Buffer implementations publish them through [`BufferReader::buffer_requirements`]
/// and [`BufferWriter::buffer_requirements`]. Before `connect()` runs, the
/// flowgraph merges peer requirements and passes them back through
/// [`BufferReader::raise_buffer_requirements`] and
/// [`BufferWriter::raise_buffer_requirements`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BufferRequirements {
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl BufferRequirements {
    /// Create empty buffer requirements.
    pub const fn new() -> Self {
        Self {
            min_items: None,
            min_buffer_size_in_items: None,
        }
    }

    /// Create buffer requirements with an initial `min_items`.
    pub const fn with_min_items(min_items: usize) -> Self {
        Self {
            min_items: Some(min_items),
            min_buffer_size_in_items: None,
        }
    }

    /// Minimum number of readable or writable items requested by the port.
    pub const fn min_items(&self) -> Option<usize> {
        self.min_items
    }

    /// Minimum buffer capacity requested by the port, in items.
    pub const fn min_buffer_size_in_items(&self) -> Option<usize> {
        self.min_buffer_size_in_items
    }

    /// Configure the minimum number of items required by the port.
    pub fn set_min_items(&mut self, min_items: usize) {
        self.min_items = Some(min_items);
    }

    /// Raise the minimum item requirement to at least `min_items`.
    pub fn raise_min_items(&mut self, min_items: usize) {
        self.min_items = Some(self.min_items.unwrap_or(0).max(min_items));
    }

    /// Configure the minimum buffer size in items.
    pub fn set_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.min_buffer_size_in_items = Some(min_items);
    }

    /// Raise the minimum buffer capacity requirement to at least `min_items`.
    pub fn raise_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.min_buffer_size_in_items =
            Some(self.min_buffer_size_in_items.unwrap_or(0).max(min_items));
    }

    /// Merge item and buffer-size requirements, keeping the larger values.
    pub fn merge(&mut self, other: Self) {
        if let Some(min_items) = other.min_items {
            self.raise_min_items(min_items);
        }
        if let Some(min_items) = other.min_buffer_size_in_items {
            self.raise_min_buffer_size_in_items(min_items);
        }
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
///
/// This selects the control-plane handle and its lightweight notifier. It does
/// not make a buffer endpoint `Send` or opt it into cross-domain connections;
/// that capability is declared separately with [`ThreadSafeConnect`].
pub trait BufferInbox: Clone + Debug + 'static {
    /// Wake-only handle stored on hot paths.
    type Notifier: BufferNotifier;

    /// Select this inbox from the handles provided during port initialization.
    fn from_port_inboxes(inboxes: &PortInboxes) -> Self;
    /// Extract a wake-only notifier from this inbox.
    fn notifier(&self) -> Self::Notifier;
    /// Wake the owning block without sending a message.
    fn notify(&self);
    /// Notify the destination block that one stream input port is done.
    fn stream_input_done(&self, input_id: PortIndex) -> impl Future<Output = Result<(), Error>>;
    /// Notify the destination block that one stream output port is done.
    fn stream_output_done(&self, output_id: PortIndex) -> impl Future<Output = Result<(), Error>>;
}

impl BufferInbox for BlockInbox {
    type Notifier = BlockNotifier;

    fn from_port_inboxes(inboxes: &PortInboxes) -> Self {
        inboxes.thread_safe_inbox()
    }

    fn notifier(&self) -> Self::Notifier {
        BlockInbox::notifier(self)
    }

    #[inline(always)]
    fn notify(&self) {
        BlockInbox::notify(self);
    }

    async fn stream_input_done(&self, input_id: PortIndex) -> Result<(), Error> {
        BlockInbox::stream_input_done(self, input_id).await
    }

    async fn stream_output_done(&self, output_id: PortIndex) -> Result<(), Error> {
        BlockInbox::stream_output_done(self, output_id).await
    }
}

impl BufferInbox for LocalBlockInbox {
    type Notifier = LocalBlockNotifier;

    fn from_port_inboxes(inboxes: &PortInboxes) -> Self {
        inboxes.local_inbox()
    }

    fn notifier(&self) -> Self::Notifier {
        LocalBlockInbox::notifier(self)
    }

    #[inline(always)]
    fn notify(&self) {
        LocalBlockInbox::notify(self);
    }

    async fn stream_input_done(&self, input_id: PortIndex) -> Result<(), Error> {
        LocalBlockInbox::send(self, BlockMessage::StreamInputDone { input_id }).await
    }

    async fn stream_output_done(&self, output_id: PortIndex) -> Result<(), Error> {
        LocalBlockInbox::send(self, BlockMessage::StreamOutputDone { output_id }).await
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_inbox_notifier<I, N>()
    where
        I: BufferInbox<Notifier = N>,
        N: BufferNotifier,
    {
    }

    #[test]
    fn built_in_inboxes_select_concrete_notifiers() {
        assert_inbox_notifier::<BlockInbox, BlockNotifier>();
        assert_inbox_notifier::<LocalBlockInbox, LocalBlockNotifier>();
    }

    #[test]
    fn erased_connection_capability_follows_thread_safe_connect_impl() {
        let writer = super::DefaultCpuWriter::<u8>::default();
        assert!(DynBufferWriter::thread_safe_connect(&writer).is_some());

        let writer = super::local::Writer::<u8>::default();
        assert!(DynBufferWriter::thread_safe_connect(&writer).is_none());
    }

    #[test]
    fn buffer_requirements_are_extracted_from_configured_ports() {
        let mut writer = super::DefaultCpuWriter::<u8>::default();
        CpuBufferWriter::set_min_items(&mut writer, 4);
        CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 32);

        let mut reader = super::DefaultCpuReader::<u8>::default();
        CpuBufferReader::set_min_items(&mut reader, 8);
        CpuBufferReader::set_min_buffer_size_in_items(&mut reader, 64);

        let writer_requirements = BufferWriter::buffer_requirements(&writer);
        let reader_requirements = BufferReader::buffer_requirements(&reader);

        assert_eq!(writer_requirements.min_items(), Some(4));
        assert_eq!(writer_requirements.min_buffer_size_in_items(), Some(32));
        assert_eq!(BufferWriter::max_readers(&writer), usize::MAX);
        assert_eq!(reader_requirements.min_items(), Some(8));
        assert_eq!(reader_requirements.min_buffer_size_in_items(), Some(64));
    }
}

/// Binding state shared by all stream ports.
#[derive(Debug, Clone)]
pub enum PortBinding<I: BufferInbox = BlockInbox> {
    /// Port is only constructed and not yet attached to a concrete block/port id.
    Unbound,
    /// Port is attached to a concrete block/port id inside a flowgraph.
    Bound {
        /// Owning block of the bound port.
        block_id: BlockId,
        /// Port id inside the owning block.
        port_index: PortIndex,
        /// Inbox used to notify the owning block.
        inbox: I,
    },
}

/// Shared per-port state that is independent from the concrete buffer backend.
#[derive(Debug, Clone)]
pub struct PortCore<I: BufferInbox = BlockInbox> {
    binding: PortBinding<I>,
    requirements: BufferRequirements,
}

impl<I: BufferInbox> PortCore<I> {
    /// Create an unbound port with empty buffer requirements.
    pub const fn new_unbound() -> Self {
        Self::with_requirements(BufferRequirements::new())
    }

    /// Create an unbound port with the provided buffer requirements.
    pub const fn with_requirements(requirements: BufferRequirements) -> Self {
        Self {
            binding: PortBinding::Unbound,
            requirements,
        }
    }

    /// Bind the port to the given block/port id and inbox.
    pub fn init(&mut self, block_id: BlockId, port_index: PortIndex, inbox: I) {
        self.binding = PortBinding::Bound {
            block_id,
            port_index,
            inbox,
        };
    }

    /// Whether the port has been bound to a block inside a flowgraph.
    pub fn is_bound(&self) -> bool {
        matches!(self.binding, PortBinding::Bound { .. })
    }

    /// The current binding state.
    pub fn binding(&self) -> &PortBinding<I> {
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
    pub fn port_id(&self) -> PortIndex {
        match &self.binding {
            PortBinding::Bound { port_index, .. } => *port_index,
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the bound port index if available.
    pub fn port_id_if_bound(&self) -> Option<PortIndex> {
        match &self.binding {
            PortBinding::Bound { port_index, .. } => Some(*port_index),
            PortBinding::Unbound => None,
        }
    }

    /// Borrow the bound inbox.
    pub fn inbox(&self) -> &I {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => inbox,
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Get the wake-only notifier associated with the bound inbox.
    pub fn notifier(&self) -> I::Notifier {
        match &self.binding {
            PortBinding::Bound { inbox, .. } => inbox.notifier(),
            PortBinding::Unbound => panic!("port is not bound to a flowgraph"),
        }
    }

    /// Create an endpoint for this port if it has been bound to a flowgraph.
    pub fn endpoint_if_bound(&self) -> Option<PortEndpoint<I>> {
        match &self.binding {
            PortBinding::Bound {
                port_index, inbox, ..
            } => Some(PortEndpoint::new(inbox.clone(), *port_index)),
            PortBinding::Unbound => None,
        }
    }

    /// Minimum number of items requested by the port.
    pub fn min_items(&self) -> Option<usize> {
        self.requirements.min_items()
    }

    /// Configure the minimum number of items required by the port.
    pub fn set_min_items(&mut self, min_items: usize) {
        self.requirements.set_min_items(min_items);
    }

    /// Raise the minimum number of items required by the port.
    pub fn raise_min_items(&mut self, min_items: usize) {
        self.requirements.raise_min_items(min_items);
    }

    /// Minimum configured buffer size in items.
    pub fn min_buffer_size_in_items(&self) -> Option<usize> {
        self.requirements.min_buffer_size_in_items()
    }

    /// Return the buffer requirements configured on this port.
    ///
    /// Custom buffer implementations can use this from
    /// [`BufferReader::buffer_requirements`] or
    /// [`BufferWriter::buffer_requirements`].
    pub fn requirements(&self) -> BufferRequirements {
        self.requirements
    }

    /// Raise this port's configured requirements to at least `requirements`.
    ///
    /// Custom buffer implementations can call this from
    /// [`BufferReader::raise_buffer_requirements`] or
    /// [`BufferWriter::raise_buffer_requirements`] before allocating or
    /// connecting their backend state.
    pub fn raise_requirements(&mut self, requirements: BufferRequirements) {
        self.requirements.merge(requirements);
    }

    /// Configure the minimum buffer size in items.
    pub fn set_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.requirements.set_min_buffer_size_in_items(min_items);
    }

    /// Raise the minimum buffer size in items.
    pub fn raise_min_buffer_size_in_items(&mut self, min_items: usize) {
        self.requirements.raise_min_buffer_size_in_items(min_items);
    }

    /// Create a validation error for an unconnected port.
    pub fn not_connected_error(&self) -> Error {
        match &self.binding {
            PortBinding::Bound {
                block_id,
                port_index,
                ..
            } => Error::ValidationError(format!("{block_id:?}:{port_index:?} not connected")),
            PortBinding::Unbound => {
                Error::ValidationError("stream port is not bound to a flowgraph".to_string())
            }
        }
    }
}

/// A peer endpoint captured during connection setup.
#[derive(Debug, Clone)]
pub struct PortEndpoint<I: BufferInbox = BlockInbox> {
    inbox: I,
    port_id: PortIndex,
}

impl<I: BufferInbox> PortEndpoint<I> {
    /// Create a new peer endpoint.
    pub fn new(inbox: I, port_id: PortIndex) -> Self {
        Self { inbox, port_id }
    }

    /// Borrow the peer inbox.
    pub fn inbox(&self) -> &I {
        &self.inbox
    }

    /// Get the peer port index.
    pub fn port_id(&self) -> PortIndex {
        self.port_id
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

/// Type-erased reader side of a stream buffer.
pub trait DynBufferReader: Any {
    /// Buffer requirements configured on this port.
    fn buffer_requirements(&self) -> BufferRequirements;
    /// Raise this port's configured requirements.
    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements);
    /// Initialize the reader from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes);
    /// Validate that this reader is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Mark this reader because the upstream writer is done.
    fn finish(&mut self);
    /// Return whether the upstream writer has marked this buffer as done.
    fn finished(&self) -> bool;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port index.
    fn port_id(&self) -> PortIndex;
}

impl<T: BufferReader> DynBufferReader for T {
    fn buffer_requirements(&self) -> BufferRequirements {
        BufferReader::buffer_requirements(self)
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        BufferReader::raise_buffer_requirements(self, requirements);
    }

    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes) {
        BufferReader::init_from(self, block_id, port_index, inboxes);
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

    fn port_id(&self) -> PortIndex {
        BufferReader::port_id(self)
    }
}

/// Reader side of a stream buffer.
pub trait BufferReader: Any {
    /// Concrete inbox handle stored by this buffer.
    type Inbox: BufferInbox;
    /// Buffer requirements configured on this port.
    fn buffer_requirements(&self) -> BufferRequirements {
        BufferRequirements::new()
    }
    /// Raise this port's configured requirements.
    fn raise_buffer_requirements(&mut self, _requirements: BufferRequirements) {}
    /// Initialize the reader with its owning block, port index, and inbox.
    fn init(&mut self, block_id: BlockId, port_index: PortIndex, inbox: Self::Inbox);
    /// Initialize the reader from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes) {
        self.init(
            block_id,
            port_index,
            Self::Inbox::from_port_inboxes(inboxes),
        );
    }
    /// Validate that this reader is connected and ready to run.
    ///
    /// The runtime calls this during flowgraph startup before any block `init()`
    /// method runs.
    fn validate(&self) -> Result<(), Error>;
    /// Notify upstream writers that this reader is done.
    ///
    /// Implementations usually forward this signal through the peer inbox handle
    /// so the upstream block can stop producing. The upstream block finishes as
    /// a whole; for a fanout writer this also stops further production for its
    /// other readers, although they may still drain already-buffered items.
    fn notify_finished(&mut self) -> impl Future<Output = ()>
    where
        Self: Sized;
    /// Mark this reader because the upstream writer is done.
    fn finish(&mut self);
    /// Return whether the upstream writer has marked this buffer as done.
    fn finished(&self) -> bool;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port index.
    fn port_id(&self) -> PortIndex;
}

/// Buffer writer that can be connected while its endpoints stay in different domains.
///
/// Connection always proceeds from reader to writer and back to the reader:
///
/// 1. [`take_reader_token`](Self::take_reader_token) creates a sendable offer in
///    the reader's domain.
/// 2. [`connect_reader`](Self::connect_reader) mutates the pinned writer and
///    returns a sendable reader installation token.
/// 3. [`finish_reader`](Self::finish_reader) installs that token into the pinned
///    reader.
pub trait ThreadSafeConnect: BufferWriter {
    /// Sendable reader offer consumed in the writer's domain.
    type ReaderToken: Send + 'static;
    /// Sendable reader installation token returned by the writer.
    type WriterToken: Send + 'static;

    /// Create an offer from the reader without moving the reader endpoint.
    fn take_reader_token(reader: &mut Self::Reader) -> Self::ReaderToken;
    /// Attach the offered reader while keeping the writer endpoint pinned.
    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken;
    /// Install the writer-created connection state into the pinned reader.
    fn finish_reader(reader: &mut Self::Reader, token: Self::WriterToken);
}

pub(crate) type DynThreadSafeToken = Box<dyn Any + Send>;

/// Type-erased adapter for a concrete [`ThreadSafeConnect`] implementation.
#[doc(hidden)]
pub trait DynThreadSafeConnect: Debug + Send + Sync {
    /// Create an erased offer from a type-erased reader.
    fn take_reader(&self, reader: &mut dyn DynBufferReader) -> Result<DynThreadSafeToken, Error>;
    /// Attach an erased reader offer to a type-erased writer.
    fn connect_reader(
        &self,
        writer: &mut dyn DynBufferWriter,
        token: DynThreadSafeToken,
    ) -> Result<DynThreadSafeToken, Error>;
    /// Install an erased writer token into a type-erased reader.
    fn finish_reader(
        &self,
        reader: &mut dyn DynBufferReader,
        token: DynThreadSafeToken,
    ) -> Result<(), Error>;
}

struct ErasedThreadSafeConnect<W>(PhantomData<fn() -> W>);

impl<W> Debug for ErasedThreadSafeConnect<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynThreadSafeConnect")
            .finish_non_exhaustive()
    }
}

impl<W> DynThreadSafeConnect for ErasedThreadSafeConnect<W>
where
    W: ThreadSafeConnect + 'static,
{
    fn take_reader(&self, reader: &mut dyn DynBufferReader) -> Result<DynThreadSafeToken, Error> {
        let reader = (reader as &mut dyn Any)
            .downcast_mut::<W::Reader>()
            .ok_or_else(|| Error::ValidationError("stream reader has wrong type".to_string()))?;
        Ok(Box::new(W::take_reader_token(reader)))
    }

    fn connect_reader(
        &self,
        writer: &mut dyn DynBufferWriter,
        token: DynThreadSafeToken,
    ) -> Result<DynThreadSafeToken, Error> {
        let writer = (writer as &mut dyn Any)
            .downcast_mut::<W>()
            .ok_or_else(|| Error::ValidationError("stream writer has wrong type".to_string()))?;
        let token = token.downcast::<W::ReaderToken>().map_err(|_| {
            Error::ValidationError("stream connection token has unexpected type".to_string())
        })?;
        Ok(Box::new(writer.connect_reader(*token)))
    }

    fn finish_reader(
        &self,
        reader: &mut dyn DynBufferReader,
        token: DynThreadSafeToken,
    ) -> Result<(), Error> {
        let reader = (reader as &mut dyn Any)
            .downcast_mut::<W::Reader>()
            .ok_or_else(|| Error::ValidationError("stream reader has wrong type".to_string()))?;
        let token = token.downcast::<W::WriterToken>().map_err(|_| {
            Error::ValidationError("stream connection token has unexpected type".to_string())
        })?;
        W::finish_reader(reader, *token);
        Ok(())
    }
}

/// Type-erased writer side of a stream buffer.
pub trait DynBufferWriter: Any {
    /// Concrete reader type identity expected by this writer.
    fn reader_type_id(&self) -> TypeId;
    /// Maximum number of downstream readers supported by this writer.
    fn max_readers(&self) -> usize;
    /// Buffer requirements configured on this port.
    fn buffer_requirements(&self) -> BufferRequirements;
    /// Raise this port's configured requirements.
    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements);
    /// Initialize the writer from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes);
    /// Validate that this writer is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Connect this writer to a type-erased reader.
    fn connect_dyn(&mut self, dest: &mut dyn DynBufferReader) -> Result<(), Error>;
    /// Type-erased cross-domain connection capability, if implemented.
    #[doc(hidden)]
    fn thread_safe_connect(&self) -> Option<Arc<dyn DynThreadSafeConnect>>;
    /// Get the owning block id.
    fn block_id(&self) -> BlockId;
    /// Get the owning port index.
    fn port_id(&self) -> PortIndex;
}

impl<T> DynBufferWriter for T
where
    T: BufferWriter + 'static,
{
    fn reader_type_id(&self) -> TypeId {
        TypeId::of::<T::Reader>()
    }

    fn max_readers(&self) -> usize {
        BufferWriter::max_readers(self)
    }

    fn buffer_requirements(&self) -> BufferRequirements {
        BufferWriter::buffer_requirements(self)
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        BufferWriter::raise_buffer_requirements(self, requirements);
    }

    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes) {
        BufferWriter::init_from(self, block_id, port_index, inboxes);
    }

    fn validate(&self) -> Result<(), Error> {
        BufferWriter::validate(self)
    }

    fn connect_dyn(&mut self, dest: &mut dyn DynBufferReader) -> Result<(), Error> {
        BufferWriter::connect_dyn(self, dest)
    }

    default fn thread_safe_connect(&self) -> Option<Arc<dyn DynThreadSafeConnect>> {
        None
    }

    fn block_id(&self) -> BlockId {
        BufferWriter::block_id(self)
    }

    fn port_id(&self) -> PortIndex {
        BufferWriter::port_id(self)
    }
}

impl<T> DynBufferWriter for T
where
    T: ThreadSafeConnect + 'static,
{
    fn thread_safe_connect(&self) -> Option<Arc<dyn DynThreadSafeConnect>> {
        Some(Arc::new(ErasedThreadSafeConnect::<T>(PhantomData)))
    }
}

/// Writer side of a stream buffer.
pub trait BufferWriter: Any {
    /// Concrete inbox handle stored by this buffer.
    type Inbox: BufferInbox;
    /// The corresponding matching reader.
    type Reader: BufferReader<Inbox = Self::Inbox>;
    /// Maximum number of downstream readers supported by this writer.
    ///
    /// Fanout readers share the writer's termination fate: when any reader
    /// finishes, the upstream block finishes and stops producing for all readers.
    /// Remaining readers may still drain items already present in their buffers.
    fn max_readers(&self) -> usize {
        1
    }
    /// Buffer requirements configured on this port.
    fn buffer_requirements(&self) -> BufferRequirements {
        BufferRequirements::new()
    }
    /// Raise this port's configured requirements.
    fn raise_buffer_requirements(&mut self, _requirements: BufferRequirements) {}
    /// Initialize the writer with its owning block, port index, and inbox.
    fn init(&mut self, block_id: BlockId, port_index: PortIndex, inbox: Self::Inbox);
    /// Initialize the writer from a block's available inbox handles.
    fn init_from(&mut self, block_id: BlockId, port_index: PortIndex, inboxes: &PortInboxes) {
        self.init(
            block_id,
            port_index,
            Self::Inbox::from_port_inboxes(inboxes),
        );
    }
    /// Validate that this writer is connected and ready to run.
    fn validate(&self) -> Result<(), Error>;
    /// Connect the writer to a matching reader.
    ///
    /// This is called while the flowgraph is being constructed, before runtime
    /// startup. Implementations should store peer queues, not transfer samples.
    fn connect(&mut self, dest: &mut Self::Reader);
    /// Connect the writer to a type-erased matching reader.
    fn connect_dyn(&mut self, dest: &mut dyn DynBufferReader) -> Result<(), Error> {
        if let Some(concrete) = (dest as &mut dyn Any).downcast_mut::<Self::Reader>() {
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
    /// Get the owning port index.
    fn port_id(&self) -> PortIndex;
}

/// Value-level contract for sample types supported by CPU buffers.
///
/// Samples are copyable, have a valid default value for initializing typed
/// storage, and occupy at least one byte. Backends that reinterpret arbitrary
/// bytes impose an additional representation bound such as `bytemuck::Pod`.
pub trait CpuSample: Copy + Default + std::fmt::Debug + Send + Sync + 'static {
    /// Non-zero size of one sample in bytes.
    const SIZE: NonZeroUsize =
        NonZeroUsize::new(size_of::<Self>()).expect("sample types must not be zero-sized");
}

impl<T> CpuSample for T where T: Copy + Default + std::fmt::Debug + Send + Sync + 'static {}

/// CPU stream reader API.
pub trait CpuBufferReader: BufferReader + Default {
    /// Item type.
    type Item: CpuSample;
    /// Get readable slice and associated tags.
    ///
    /// The returned slice starts at the next unread item. Tags use indices
    /// relative to this slice.
    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]);
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

/// CPU stream writer API.
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
}

/// Writer half of an in-place circuit buffer.
pub trait InplaceWriter: BufferWriter + Default {
    /// Item type carried by this in-place writer.
    type Item: CpuSample;
    /// Buffer chunk type moved through this writer.
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Submit a full buffer to the downstream reader.
    fn put_full_buffer(&mut self, buffer: Self::Buffer) -> Result<(), Error>;

    /// Get an empty buffer, if one is available.
    ///
    /// This is typically used by source-style blocks that create data at the
    /// beginning of an in-place circuit.
    fn get_empty_buffer(&mut self) -> Option<Self::Buffer>;
    /// Return whether more empty buffers are immediately available.
    fn has_more_buffers(&mut self) -> bool;
    /// Inject new empty buffers using the configured default item capacity.
    fn inject_buffers(&mut self, n_buffers: usize) {
        let n_items = config().buffer_size / Self::Item::SIZE.get();
        self.inject_buffers_with_items(n_buffers, n_items);
    }
    /// Inject new empty buffers with an explicit item capacity.
    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize);
}

#[cfg(not(target_arch = "wasm32"))]
/// Default native [`CpuBufferReader`] implementation.
pub type DefaultCpuReader<D> = circular::Reader<D>;
/// Default native [`CpuBufferWriter`] implementation.
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
