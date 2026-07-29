//! Stream buffer traits and built-in buffer implementations.
//!
//! Buffers are the transport layer between stream ports. Application and block
//! code uses the buffer families and traits in this module. Support types for
//! implementing a custom buffer backend are grouped under [`dev`].
//!
//! # Stream termination and fanout
//!
//! When a block finishes, each of its input readers notifies the upstream block
//! that the corresponding writer is no longer needed. The upstream block then
//! finishes as a whole and notifies every reader connected to all of its output
//! writers. For a writer with multiple readers, one reader finishing therefore
//! stops further production for every reader. The other readers may still
//! process items already present in their buffers.

mod core;

#[cfg(feature = "burn")]
pub use self::core::burn;
pub use self::core::circuit;
#[cfg(not(target_arch = "wasm32"))]
pub use self::core::circular;
pub use self::core::local;
#[doc(hidden)]
pub use self::core::queued;
pub use self::core::slab;
#[cfg(feature = "wgpu")]
pub use self::core::wgpu;

pub use self::core::BufferReader;
pub use self::core::BufferWriter;
pub use self::core::CpuBufferReader;
pub use self::core::CpuBufferWriter;
pub use self::core::CpuSample;
pub use self::core::DefaultCpuReader;
pub use self::core::DefaultCpuWriter;
pub use self::core::InplaceBuffer;
pub use self::core::InplaceReader;
pub use self::core::InplaceWriter;
pub use self::core::LocalCpuReader;
pub use self::core::LocalCpuWriter;
pub use self::core::Tags;
pub use self::core::ThreadSafeConnect;

pub(crate) use self::core::BlockInbox;
pub(crate) use self::core::BufferInbox;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use self::core::BufferNotifier;
pub(crate) use self::core::BufferRequirements;
pub(crate) use self::core::CacheAlignedBuffer;
pub(crate) use self::core::CircuitReturn;
pub(crate) use self::core::ConnectionState;
pub(crate) use self::core::DynBufferReader;
pub(crate) use self::core::DynBufferWriter;
pub(crate) use self::core::DynThreadSafeConnect;
pub(crate) use self::core::DynThreadSafeToken;
pub(crate) use self::core::LocalBlockInbox;
pub(crate) use self::core::PortCore;
pub(crate) use self::core::PortEndpoint;
pub(crate) use self::core::PortInboxes;

/// Support types for implementing custom stream buffers.
pub mod dev {
    pub use super::core::BlockInbox;
    pub use super::core::BufferInbox;
    pub use super::core::BufferNotifier;
    pub use super::core::BufferRequirements;
    pub use super::core::CacheAlignedBuffer;
    pub use super::core::ConnectionState;
    pub use super::core::DynBufferReader;
    pub use super::core::DynBufferWriter;
    pub use super::core::DynThreadSafeConnect;
    pub use super::core::LocalBlockInbox;
    pub use super::core::PortCore;
    pub use super::core::PortEndpoint;
    pub use super::core::PortInboxes;
}
