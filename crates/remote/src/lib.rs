#![warn(missing_docs)]
//! # Remote interaction with FutureSDR
//!
//! Library for remote interaction with a FutureSDR runtime and flowgraph.
//!
//! ## Example
//! ```no_run
//! use futuresdr_remote::Error;
//! use futuresdr_remote::Handler;
//! use futuresdr_remote::Remote;
//! use futuresdr_types::Pmt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let remote = Remote::new("http://127.0.0.1:1337");
//!
//!     let fgs = remote.flowgraphs().await?;
//!     let blocks = fgs[0].blocks();
//!
//!     let p = blocks[0].callback(Handler::Id(0), Pmt::U32(123)).await?;
//!     println!("result: {:?}", p);
//!
//!     Ok(())
//! }
//! ```
mod remote;
pub use futuresdr_types as types;
pub use remote::Block;
pub use remote::Connection;
pub use remote::ConnectionType;
pub use remote::Flowgraph;
pub use remote::Handler;
pub use remote::Remote;

use thiserror::Error;

/// FutureSDR Remote Error
#[derive(Debug, Error)]
pub enum Error {
    /// Error in [`hyper`] crate.
    #[error("Reqwest")]
    Reqwest(#[from] reqwest::Error),
    /// Wrong [`Flowgraph`] ID.
    #[error("Wrong flowgraph id")]
    FlowgraphId(usize),
}
