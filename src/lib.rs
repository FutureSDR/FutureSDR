#![warn(missing_docs)]
#![recursion_limit = "512"]

//! An experimental asynchronous SDR runtime for heterogeneous architectures that is:
//! * **Extensible**: custom buffers (supporting accelerators like GPUs and FPGAs) and custom schedulers (optimized for your application).
//! * **Asynchronous**: solving long-standing issues around IO, blocking, and timers.
//! * **Portable**: Linux, Windows, Mac, WASM, Android, and prime support for embedded platforms through a REST API and web-based GUIs.
//! * **Fast**: SDR go brrr!
//!
//! ## Example
//! An example flowgraph that forwards 123 zeros into a sink:
//! ```
//! use futuresdr::blocks::Head;
//! use futuresdr::blocks::NullSink;
//! use futuresdr::blocks::NullSource;
//! use futuresdr::prelude::*;
//!
//! fn main() -> Result<()> {
//!     let mut fg = Flowgraph::new();
//!
//!     let src = NullSource::<u8>::new();
//!     let head = Head::<u8>::new(123);
//!     let snk = NullSink::<u8>::new();
//!
//!     connect!(fg, src > head > snk);
//!
//!     Runtime::new().run(fg)?;
//!
//!     Ok(())
//! }
//! ```

// make the futuresdr crate available in the library to allow referencing it as
// futuresdr internally, which simpilifies proc macros.
extern crate self as futuresdr;
#[macro_use]
extern crate futuresdr_macros;
/// Logging macro
#[macro_use]
pub extern crate tracing;

// re-exports
#[cfg(not(target_arch = "wasm32"))]
pub use async_io;
#[cfg(not(target_arch = "wasm32"))]
pub use async_net;
pub use futuredsp;
pub use futures;
#[cfg(all(feature = "audio", not(target_arch = "wasm32")))]
pub use hound;
pub use num_complex;
pub use num_integer;
#[cfg(feature = "seify")]
pub use seify;

pub mod blocks;
pub mod runtime;
/// FutureSDR Async Channels
///
/// At the moment this uses the channels from the `futures` crate.
pub mod channel {
    pub use futures::channel::mpsc;
    pub use futures::channel::oneshot;
}

/// Macros
pub mod macros {
    #[doc(hidden)]
    pub use async_trait::async_trait as async_trait_orig;

    pub use futuresdr_macros::Block;
    pub use futuresdr_macros::async_trait;
    pub use futuresdr_macros::connect;
}

/// Prelude with common structs and traits
pub mod prelude {
    pub use futures::prelude::*;
    pub use futuresdr::channel::mpsc;
    pub use futuresdr::channel::oneshot;
    pub use futuresdr::macros::Block;
    pub use futuresdr::macros::async_trait;
    pub use futuresdr::macros::connect;
    pub use futuresdr::runtime::BlockId;
    pub use futuresdr::runtime::BlockMeta;
    pub use futuresdr::runtime::BlockRef;
    pub use futuresdr::runtime::Error;
    pub use futuresdr::runtime::Flowgraph;
    pub use futuresdr::runtime::FlowgraphHandle;
    pub use futuresdr::runtime::FlowgraphId;
    pub use futuresdr::runtime::ItemTag;
    pub use futuresdr::runtime::Kernel;
    pub use futuresdr::runtime::MessageOutputs;
    pub use futuresdr::runtime::Pmt;
    pub use futuresdr::runtime::PortId;
    pub use futuresdr::runtime::Result;
    pub use futuresdr::runtime::Runtime;
    pub use futuresdr::runtime::RuntimeHandle;
    pub use futuresdr::runtime::Tag;
    pub use futuresdr::runtime::WorkIo;
    pub use futuresdr::runtime::buffer::BufferReader;
    pub use futuresdr::runtime::buffer::BufferWriter;
    pub use futuresdr::runtime::buffer::CpuBufferReader;
    pub use futuresdr::runtime::buffer::CpuBufferWriter;
    pub use futuresdr::runtime::buffer::CpuSample;
    pub use futuresdr::runtime::buffer::DefaultCpuReader;
    pub use futuresdr::runtime::buffer::DefaultCpuWriter;
    pub use futuresdr::runtime::buffer::InplaceBuffer;
    pub use futuresdr::runtime::buffer::InplaceReader;
    pub use futuresdr::runtime::buffer::InplaceWriter;
    #[cfg(feature = "burn")]
    pub use futuresdr::runtime::buffer::burn as burn_buffer;
    pub use futuresdr::runtime::buffer::circuit;
    #[cfg(not(target_arch = "wasm32"))]
    pub use futuresdr::runtime::buffer::circular;
    pub use futuresdr::runtime::buffer::slab;
    pub use futuresdr::tracing::debug;
    pub use futuresdr::tracing::error;
    pub use futuresdr::tracing::info;
    pub use futuresdr::tracing::trace;
    pub use futuresdr::tracing::warn;
    pub use num_complex::*;
}
