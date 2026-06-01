//! Developer-facing runtime APIs for implementing custom blocks, buffers, and
//! runtime extensions.
//!
//! App authors building and running flowgraphs should generally prefer
//! [`crate::prelude`] and the top-level [`crate::runtime`] APIs.

pub use super::block::Block;
#[doc(hidden)]
pub use super::block::BlockObject;
pub use super::block_inbox::BlockInbox;
pub use super::block_inbox::BlockNotifier;
pub use super::block_inbox::LocalBlockInbox;
pub use super::block_inbox::LocalBlockNotifier;
pub use super::block_inbox::ThreadSafeBlockInbox;
pub use super::block_meta::BlockMeta;
pub use super::buffer::BufferReader;
pub use super::buffer::BufferWriter;
pub use super::buffer::CpuBufferReader;
pub use super::buffer::CpuBufferWriter;
pub use super::buffer::CpuSample;
pub use super::buffer::DefaultCpuReader;
pub use super::buffer::DefaultCpuWriter;
pub use super::buffer::InplaceBuffer;
pub use super::buffer::InplaceReader;
pub use super::buffer::InplaceWriter;
pub use super::buffer::LocalCpuReader;
pub use super::buffer::LocalCpuWriter;
pub use super::buffer::PortInboxes;
pub use super::buffer::SendBufferReader;
pub use super::buffer::SendBufferWriter;
pub use super::buffer::SendCpuBufferReader;
pub use super::buffer::SendCpuBufferWriter;
pub use super::buffer::SendInplaceReader;
pub use super::buffer::SendInplaceWriter;
pub use super::flowgraph::TypedBlockGuard;
pub use super::flowgraph::TypedBlockGuardMut;
pub use super::kernel::Kernel;
#[doc(hidden)]
pub use super::kernel::SendKernel;
pub use super::message_output::MessageOutputs;
pub use super::tag::ItemTag;
pub use super::tag::Tag;
pub use super::work_io::WorkIo;

/// Prelude for implementing custom blocks and runtime extensions.
///
/// This prelude includes the application-facing [`crate::prelude`] plus the
/// traits, buffer types, message helpers, tags, and macros needed by custom
/// block implementations.
pub mod prelude {
    pub use crate::prelude::*;
    pub use crate::runtime::buffer::PortInboxes;
    #[cfg(feature = "burn")]
    pub use crate::runtime::buffer::burn as burn_buffer;
    pub use crate::runtime::buffer::circuit;
    #[cfg(not(target_arch = "wasm32"))]
    pub use crate::runtime::buffer::circular;
    pub use crate::runtime::buffer::slab;
    pub use crate::runtime::channel::mpsc;
    pub use crate::runtime::channel::oneshot;
    pub use crate::runtime::dev::Block;
    pub use crate::runtime::dev::BlockInbox;
    pub use crate::runtime::dev::BlockMeta;
    pub use crate::runtime::dev::BlockNotifier;
    #[doc(hidden)]
    pub use crate::runtime::dev::BlockObject;
    pub use crate::runtime::dev::BufferReader;
    pub use crate::runtime::dev::BufferWriter;
    pub use crate::runtime::dev::CpuBufferReader;
    pub use crate::runtime::dev::CpuBufferWriter;
    pub use crate::runtime::dev::CpuSample;
    pub use crate::runtime::dev::DefaultCpuReader;
    pub use crate::runtime::dev::DefaultCpuWriter;
    pub use crate::runtime::dev::InplaceBuffer;
    pub use crate::runtime::dev::InplaceReader;
    pub use crate::runtime::dev::InplaceWriter;
    pub use crate::runtime::dev::ItemTag;
    pub use crate::runtime::dev::Kernel;
    pub use crate::runtime::dev::LocalBlockInbox;
    pub use crate::runtime::dev::LocalBlockNotifier;
    pub use crate::runtime::dev::LocalCpuReader;
    pub use crate::runtime::dev::LocalCpuWriter;
    pub use crate::runtime::dev::MessageOutputs;
    pub use crate::runtime::dev::SendInplaceReader;
    pub use crate::runtime::dev::SendInplaceWriter;
    pub use crate::runtime::dev::Tag;
    pub use crate::runtime::dev::ThreadSafeBlockInbox;
    pub use crate::runtime::dev::TypedBlockGuard;
    pub use crate::runtime::dev::TypedBlockGuardMut;
    pub use crate::runtime::dev::WorkIo;
    pub use crate::runtime::macros::Block;
}
