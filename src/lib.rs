#![warn(missing_docs)]
#![recursion_limit = "512"]
#![allow(clippy::new_ret_no_self)]
#![cfg_attr(docsrs, feature(doc_cfg))]

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
//! use futuresdr::macros::connect;
//! use futuresdr::runtime::Error;
//! use futuresdr::runtime::Flowgraph;
//! use futuresdr::runtime::Runtime;
//!
//! fn main() -> Result<(), Error> {
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
pub use futures_lite;
pub use num_complex;
pub use num_integer;
#[cfg(feature = "seify")]
pub use seify;

pub mod blocks;
pub mod runtime;

/// Macros
pub mod macros {
    #[doc(hidden)]
    pub use async_trait::async_trait as async_trait_orig;

    pub use futuresdr_macros::async_trait_external as async_trait;
    pub use futuresdr_macros::connect;
    pub use futuresdr_macros::message_handler_external as message_handler;
}
