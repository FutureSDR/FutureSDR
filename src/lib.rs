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
/// `mpsc` uses `kanal`, while `oneshot` uses the channels from the `futures` crate.
pub mod channel {
    /// Multi-producer, single-consumer channels backed by `kanal`.
    pub mod mpsc {
        use std::fmt;
        use std::sync::Arc;

        pub use ::kanal::ReceiveError;
        pub use ::kanal::SendError;

        /// Sending side of a channel.
        #[derive(Debug)]
        pub struct Sender<T>(Arc<::kanal::AsyncSender<T>>);

        /// Receiving side of a channel.
        #[derive(Debug)]
        pub struct Receiver<T>(::kanal::AsyncReceiver<T>);

        /// Sending side of an unbounded channel.
        pub type UnboundedSender<T> = Sender<T>;
        /// Receiving side of an unbounded channel.
        pub type UnboundedReceiver<T> = Receiver<T>;

        /// Error returned by [`Receiver::try_recv`].
        #[derive(Debug, PartialEq, Eq)]
        pub enum TryRecvError {
            /// The channel is empty but still connected.
            Empty,
            /// The channel is disconnected.
            Disconnected,
        }

        /// Error returned by [`Sender::try_send`].
        #[derive(Debug, PartialEq, Eq)]
        pub enum TrySendError<T> {
            /// The channel is full.
            Full(T),
            /// The channel is disconnected.
            Disconnected(T),
        }

        impl fmt::Display for TryRecvError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Empty => write!(f, "receive failed because channel is empty"),
                    Self::Disconnected => {
                        write!(f, "receive failed because sender dropped unexpectedly")
                    }
                }
            }
        }

        impl std::error::Error for TryRecvError {}

        impl<T> fmt::Display for TrySendError<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Full(_) => write!(f, "send failed because channel is full"),
                    Self::Disconnected(_) => {
                        write!(f, "send failed because receiver dropped unexpectedly")
                    }
                }
            }
        }

        impl<T: fmt::Debug> std::error::Error for TrySendError<T> {}

        /// Create a bounded channel.
        pub fn channel<T>(size: usize) -> (Sender<T>, Receiver<T>) {
            let (tx, rx) = ::kanal::bounded_async(size);
            (Sender(Arc::new(tx)), Receiver(rx))
        }

        /// Create an unbounded channel.
        pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
            let (tx, rx) = ::kanal::unbounded_async();
            (Sender(Arc::new(tx)), Receiver(rx))
        }

        impl<T> Clone for Sender<T> {
            fn clone(&self) -> Self {
                Self(self.0.clone())
            }
        }

        impl<T> Sender<T> {
            /// Send a value into the channel.
            pub async fn send(&self, data: T) -> Result<(), SendError> {
                self.0.send(data).await
            }

            /// Attempt to send a value without waiting.
            pub fn try_send(&self, data: T) -> Result<(), TrySendError<T>> {
                let mut data = Some(data);
                match self.0.try_send_option(&mut data) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(TrySendError::Full(data.expect("send data lost"))),
                    Err(_) => Err(TrySendError::Disconnected(data.expect("send data lost"))),
                }
            }

            /// Send a value through an unbounded channel from synchronous code.
            pub fn unbounded_send(&self, data: T) -> Result<(), SendError> {
                self.0.as_sync().send(data)
            }

            /// Return whether the receiver side has been closed.
            pub fn is_closed(&self) -> bool {
                self.0.is_disconnected() || self.0.is_closed()
            }

            /// Close the channel.
            pub async fn close(&self) -> Result<(), SendError> {
                self.0.close().map_err(|_| SendError::Closed)
            }

            /// Return whether two senders point to the same receiver.
            pub fn same_receiver(&self, other: &Self) -> bool {
                Arc::ptr_eq(&self.0, &other.0)
            }
        }

        impl<T> Receiver<T> {
            /// Receive the next value from the channel.
            pub async fn recv(&self) -> Option<T> {
                self.0.recv().await.ok()
            }

            /// Attempt to receive a value without waiting.
            pub fn try_recv(&self) -> Result<T, TryRecvError> {
                match self.0.try_recv() {
                    Ok(Some(v)) => Ok(v),
                    Ok(None) => Err(TryRecvError::Empty),
                    Err(_) => Err(TryRecvError::Disconnected),
                }
            }
        }
    }

    pub use futures::channel::oneshot;
}

/// Macros
pub mod macros {
    #[doc(hidden)]
    pub use async_trait::async_trait as async_trait_orig;

    pub use futuresdr_macros::Block;
    pub use futuresdr_macros::MegaBlock;
    pub use futuresdr_macros::async_trait;
    pub use futuresdr_macros::connect;
}

/// Prelude with common structs and traits
pub mod prelude {
    pub use futures::prelude::*;
    pub use futuresdr::channel::mpsc;
    pub use futuresdr::channel::oneshot;
    pub use futuresdr::macros::Block;
    pub use futuresdr::macros::MegaBlock;
    pub use futuresdr::macros::async_trait;
    pub use futuresdr::macros::connect;
    pub use futuresdr::runtime::BlockId;
    pub use futuresdr::runtime::BlockMeta;
    pub use futuresdr::runtime::BlockRef;
    pub use futuresdr::runtime::DynPortAccess;
    pub use futuresdr::runtime::Error;
    pub use futuresdr::runtime::Flowgraph;
    pub use futuresdr::runtime::FlowgraphHandle;
    pub use futuresdr::runtime::FlowgraphId;
    pub use futuresdr::runtime::ItemTag;
    pub use futuresdr::runtime::Kernel;
    pub use futuresdr::runtime::MegaBlock;
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
