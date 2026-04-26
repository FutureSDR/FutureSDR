//! Developer-facing runtime APIs for implementing custom blocks and runtime
//! extensions.
//!
//! App authors building and running flowgraphs should generally prefer
//! [`crate::prelude`] and the top-level [`crate::runtime`] APIs.

pub use super::block::Block;
pub use super::block_inbox::BlockInbox;
pub use super::block_inbox::BlockNotifier;
pub use super::block_meta::BlockMeta;
pub use super::buffer::BufferReader;
pub use super::buffer::BufferWriter;
pub use super::buffer::CircuitWriter;
pub use super::buffer::CpuBufferReader;
pub use super::buffer::CpuBufferWriter;
pub use super::buffer::CpuSample;
pub use super::buffer::DefaultCpuReader;
pub use super::buffer::DefaultCpuWriter;
pub use super::buffer::InplaceBuffer;
pub use super::buffer::InplaceReader;
pub use super::buffer::InplaceWriter;
pub use super::flowgraph::TypedBlockGuard;
pub use super::flowgraph::TypedBlockGuardMut;
pub use super::kernel::Kernel;
pub use super::message_output::MessageOutputs;
pub use super::tag::ItemTag;
pub use super::tag::Tag;
pub use super::work_io::WorkIo;

/// Marker trait for values that must be `Send` on native runtimes but not on wasm.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}

/// Marker trait for values that must be `Send` on native runtimes but not on wasm.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// Prelude for implementing custom blocks and other developer-facing runtime
/// extensions.
pub mod prelude {
    pub use crate::prelude::*;
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
    pub use crate::runtime::dev::BufferReader;
    pub use crate::runtime::dev::BufferWriter;
    pub use crate::runtime::dev::CircuitWriter;
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
    pub use crate::runtime::dev::MaybeSend;
    pub use crate::runtime::dev::MessageOutputs;
    pub use crate::runtime::dev::Tag;
    pub use crate::runtime::dev::TypedBlockGuard;
    pub use crate::runtime::dev::TypedBlockGuardMut;
    pub use crate::runtime::dev::WorkIo;
    pub use crate::runtime::macros::Block;
    pub use crate::runtime::macros::async_trait;
}
