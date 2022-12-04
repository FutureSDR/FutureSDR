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
pub use block::WorkIo;
pub use block_meta::BlockMeta;
pub use block_meta::BlockMetaBuilder;
pub use flowgraph::Flowgraph;
pub use flowgraph::FlowgraphHandle;
pub use flowgraph::PortId;
pub use futuresdr_pmt::Pmt;
pub use message_io::MessageInput;
pub use message_io::MessageIo;
pub use message_io::MessageIoBuilder;
pub use message_io::MessageOutput;
pub use mocker::Mocker;
pub(crate) use runtime::run_block;
pub use runtime::Runtime;
pub use stream_io::StreamInput;
pub use stream_io::StreamIo;
pub use stream_io::StreamIoBuilder;
pub use stream_io::StreamOutput;
pub use tag::ItemTag;
pub use tag::Tag;
pub use topology::Topology;

pub use futuresdr_pmt::BlockDescription;
pub use futuresdr_pmt::FlowgraphDescription;

use buffer::BufferReader;
use buffer::BufferWriter;

pub fn init() {
    logging::init();
}

#[derive(Debug)]
pub enum FlowgraphMessage {
    Terminate,
    Initialized,
    BlockDone {
        block_id: usize,
        block: Block,
    },
    BlockError {
        block_id: usize,
        block: Block,
    },
    BlockCall {
        block_id: usize,
        port_id: PortId,
        data: Pmt,
        tx: oneshot::Sender<result::Result<(), CallbackError>>,
    },
    BlockCallback {
        block_id: usize,
        port_id: PortId,
        data: Pmt,
        tx: oneshot::Sender<result::Result<Pmt, CallbackError>>,
    },
    FlowgraphDescription {
        tx: oneshot::Sender<FlowgraphDescription>,
    },
    BlockDescription {
        block_id: usize,
        tx: oneshot::Sender<result::Result<BlockDescription, BlockDescriptionError>>,
    },
}

#[derive(Debug)]
pub enum BlockMessage {
    Initialize,
    Terminate,
    Notify,
    BlockDescription {
        tx: oneshot::Sender<BlockDescription>,
    },
    StreamOutputInit {
        src_port: usize,
        writer: BufferWriter,
    },
    StreamInputInit {
        dst_port: usize,
        reader: BufferReader,
    },
    StreamInputDone {
        input_id: usize,
    },
    StreamOutputDone {
        output_id: usize,
    },
    MessageOutputConnect {
        src_port: usize,
        dst_port: usize,
        dst_inbox: mpsc::Sender<BlockMessage>,
    },
    Call {
        port_id: PortId,
        data: Pmt,
        tx: Option<oneshot::Sender<result::Result<(), HandlerError>>>,
    },
    Callback {
        port_id: PortId,
        data: Pmt,
        tx: oneshot::Sender<result::Result<Pmt, HandlerError>>,
    },
}

#[derive(Error, Debug)]
pub enum HandlerError {
    #[error("Handler does not exist")]
    InvalidHandler,
    #[error("Error in handler")]
    HandlerError,
}

#[derive(Error, Debug)]
pub enum CallbackError {
    #[error("Block does not exist")]
    InvalidBlock,
    #[error("Handler does not exist")]
    InvalidHandler,
    #[error("Error in handler")]
    HandlerError,
    #[error("Error in runtime")]
    RuntimeError,
}

impl From<HandlerError> for CallbackError {
    fn from(h: HandlerError) -> Self {
        match h {
            HandlerError::HandlerError => CallbackError::HandlerError,
            HandlerError::InvalidHandler => CallbackError::InvalidHandler,
        }
    }
}

#[derive(Error, Debug)]
pub enum BlockDescriptionError {
    #[error("Block does not exist")]
    InvalidBlock,
    #[error("Runtime Error.")]
    RuntimeError,
}
