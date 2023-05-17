//! ## SDR Runtime
use futures::channel::mpsc;
use futures::channel::oneshot;
use std::result;
use thiserror::Error;

mod block;
mod block_meta;
pub mod buffer;
pub mod config;

#[cfg(not(target_arch = "wasm32"))]
mod ctrl_port;
#[cfg(target_arch = "wasm32")]
#[path = "ctrl_port_wasm.rs"]
mod ctrl_port;
use crate::runtime::ctrl_port::ControlPort;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod logging;
#[cfg(target_os = "android")]
#[path = "logging_android.rs"]
mod logging;
#[cfg(target_arch = "wasm32")]
#[path = "logging_wasm.rs"]
mod logging;

mod flowgraph;
pub mod message_io;
mod mocker;
#[allow(clippy::module_inception)]
mod runtime;
pub mod scheduler;
pub mod stream_io;
mod tag;
mod topology;

pub use block::Block;
pub use block::Kernel;
pub use block::TypedBlock;
pub use block::WorkIo;
pub use block_meta::BlockMeta;
pub use block_meta::BlockMetaBuilder;
pub use flowgraph::Flowgraph;
pub use flowgraph::FlowgraphHandle;
pub use flowgraph::PortId;
pub use message_io::MessageInput;
pub use message_io::MessageIo;
pub use message_io::MessageIoBuilder;
pub use message_io::MessageOutput;
pub use mocker::Mocker;
pub use runtime::Runtime;
pub use runtime::RuntimeHandle;
pub use stream_io::StreamInput;
pub use stream_io::StreamIo;
pub use stream_io::StreamIoBuilder;
pub use stream_io::StreamOutput;
pub use tag::ItemTag;
pub use tag::Tag;
pub use topology::Topology;

pub use futuresdr_types::BlockDescription;
pub use futuresdr_types::FlowgraphDescription;
pub use futuresdr_types::Pmt;

use buffer::BufferReader;
use buffer::BufferWriter;

/// Initialize runtime
///
/// This function does not have to be called. Once a [`Runtime`] is started,
/// this function is called automatically.
///
/// At the moment, this only enables logging. Calling it manually, allows using
/// FutureSDR logging before a [`Runtime`] is started.
///
pub fn init() {
    logging::init();
}

/// Flowgraph inbox message type
#[derive(Debug)]
pub enum FlowgraphMessage {
    /// Terminate
    Terminate,
    /// Initialize
    Initialized,
    /// Block is done
    BlockDone {
        /// Block Id
        block_id: usize,
        /// Block
        block: Block,
    },
    /// Block encountered an error
    BlockError {
        /// BlockId
        block_id: usize,
        /// Block
        block: Block,
    },
    /// Call handler of block (ignoring result)
    BlockCall {
        /// Block Id
        block_id: usize,
        /// Message handler Id
        port_id: PortId,
        /// Input data
        data: Pmt,
        /// Back channel for result
        tx: oneshot::Sender<result::Result<(), Error>>,
    },
    /// Call handler of block
    BlockCallback {
        /// Block Id
        block_id: usize,
        /// Message handler Id
        port_id: PortId,
        /// Input data
        data: Pmt,
        /// Back channel for result
        tx: oneshot::Sender<result::Result<Pmt, Error>>,
    },
    /// Get [`FlowgraphDescription`]
    FlowgraphDescription {
        /// Back channel for result
        tx: oneshot::Sender<FlowgraphDescription>,
    },
    /// Get [`BlockDescription`]
    BlockDescription {
        /// Block Id
        block_id: usize,
        /// Back channel for result
        tx: oneshot::Sender<result::Result<BlockDescription, Error>>,
    },
}

/// Block inbox message type
#[derive(Debug)]
pub enum BlockMessage {
    /// Initialize
    Initialize,
    /// Terminate
    Terminate,
    /// Notify
    Notify,
    /// Get [`BlockDescription`]
    BlockDescription {
        /// Channel for return value
        tx: oneshot::Sender<BlockDescription>,
    },
    /// Initialize [`StreamOutput`]
    StreamOutputInit {
        /// Stream output ID
        src_port: usize,
        /// [`BufferWriter`]
        writer: BufferWriter,
    },
    /// Initialize [`StreamInput`]
    StreamInputInit {
        /// Stream input Id
        dst_port: usize,
        /// [`BufferReader`]
        reader: BufferReader,
    },
    /// Stream input port is done
    StreamInputDone {
        /// Stream input Id
        input_id: usize,
    },
    /// Stream output port is done
    StreamOutputDone {
        /// Stream output Id
        output_id: usize,
    },
    /// Connect message output
    MessageOutputConnect {
        /// Message output port Id
        src_port: usize,
        /// Destination input port Id
        dst_port: usize,
        /// Destination block inbox
        dst_inbox: mpsc::Sender<BlockMessage>,
    },
    /// Call handler (return value is ignored)
    Call {
        /// Message handler Id
        port_id: PortId,
        /// [`Pmt`] input data
        data: Pmt,
    },
    /// Call handler
    Callback {
        /// Message handler Id
        port_id: PortId,
        /// [`Pmt`] input data
        data: Pmt,
        /// Back channel for handler result
        tx: oneshot::Sender<result::Result<Pmt, Error>>,
    },
}

/// FutureSDR Error
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Block does not exist
    #[error("Block does not exist")]
    InvalidBlock,
    /// Handler does not exist
    #[error("Handler does not exist (Id {0:?})")]
    InvalidHandler(PortId),
    /// Error in Handler
    #[error("Error in handler")]
    HandlerError,
    /// Block is already terminated
    #[error("Block already terminated")]
    BlockTerminated,
    /// Runtime error
    #[error("Error in runtime")]
    RuntimeError,
}
