#![recursion_limit = "512"]
#![allow(clippy::new_ret_no_self)]
#![cfg_attr(not(RUSTC_IS_STABLE), feature(core_intrinsics))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! An experimental asynchronous SDR runtime for heterogeneous architectures that is:
//! * **Extensible**: custom buffers (supporting accelerators like GPUs and FPGAs) and custom schedulers (optimized for your application).
//! * **Asynchronous**: solving long-standing issues around IO, blocking, and timers.
//! * **Portable**: Linux, Windows, Mac, WASM, Android, and prime support for embedded platforms through a REST API and web-based GUIs.
//! * **Fast**: SDR go brrr!
//!
//! ## Example
//! An example flowgraph with a periodic message source, sending five messages to a sink:
//! ```
//! use anyhow::Result;
//! use std::time::Duration;
//!
//! use futuresdr::blocks::MessageSink;
//! use futuresdr::blocks::MessageSource;
//! use futuresdr::runtime::Flowgraph;
//! use futuresdr::runtime::Pmt;
//! use futuresdr::runtime::Runtime;
//!
//! fn main() -> Result<()> {
//!     let mut fg = Flowgraph::new();
//!
//!     let src = fg.add_block(MessageSource::new(Pmt::Null, Duration::from_secs(1), Some(5)));
//!     let snk = fg.add_block(MessageSink::new());
//!
//!     fg.connect_message(src, "out", snk, "in")?;
//!
//!     Runtime::new().run(fg)?;
//!
//!     Ok(())
//! }
//! ```

pub mod blocks;
pub mod runtime;

// re-exports
#[cfg(not(target_arch = "wasm32"))]
pub use async_io;
#[cfg(not(target_arch = "wasm32"))]
pub use async_net;
#[macro_use]
pub extern crate async_trait;
pub use futures;
pub use futures_lite;
#[macro_use]
pub extern crate log;

pub use anyhow;
pub use num_complex;
pub use num_integer;
