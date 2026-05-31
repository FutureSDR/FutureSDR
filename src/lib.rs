#![warn(missing_docs)]
#![feature(return_type_notation)]
#![feature(associated_type_defaults)]
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
//!
//! ## Main Entry Points
//! - [`blocks`] Library of common blocks that are not tied to a specific technology.
//! - [`runtime`] Runtime APIs for constructing, running, and interacting with flowgraphs.
//! - [`prelude`] Imports for constructing, running, and interacting with flowgraphs.
//!
//! ## Custom Blocks
//! To implement custom blocks or other runtime extensions, use
//! [`runtime::dev::prelude`].

// make the futuresdr crate available in the library to allow referencing it as
// futuresdr internally, which simplifies proc macros.
extern crate self as futuresdr;
/// Logging macro
#[macro_use]
pub extern crate tracing;

// re-exports
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

/// Library of common blocks that are not tied to a specific technology.
pub mod blocks;
pub mod runtime;

/// Prelude for building and controlling flowgraphs.
///
/// These imports cover the usual application-facing API: flowgraph
/// construction, runtime execution, message passing, common IDs, and logging.
/// Custom block implementations should use [`crate::runtime::dev::prelude`]
/// instead.
pub mod prelude {
    pub use futures::prelude::*;
    pub use futuresdr::runtime::BlockId;
    pub use futuresdr::runtime::BlockRef;
    pub use futuresdr::runtime::Error;
    pub use futuresdr::runtime::Flowgraph;
    pub use futuresdr::runtime::FlowgraphBlockHandle;
    pub use futuresdr::runtime::FlowgraphHandle;
    pub use futuresdr::runtime::FlowgraphId;
    pub use futuresdr::runtime::LocalDomain;
    pub use futuresdr::runtime::LocalDomainContext;
    pub use futuresdr::runtime::Pmt;
    pub use futuresdr::runtime::PortId;
    pub use futuresdr::runtime::Result;
    pub use futuresdr::runtime::RunningFlowgraph;
    pub use futuresdr::runtime::Runtime;
    pub use futuresdr::runtime::Timer;
    #[cfg(not(target_arch = "wasm32"))]
    pub use futuresdr::runtime::block_on;
    pub use futuresdr::runtime::channel::mpsc;
    pub use futuresdr::runtime::macros::connect;
    pub use futuresdr::runtime::macros::connect_async;
    pub use futuresdr::tracing::debug;
    pub use futuresdr::tracing::error;
    pub use futuresdr::tracing::info;
    pub use futuresdr::tracing::trace;
    pub use futuresdr::tracing::warn;
    pub use num_complex::*;
}
