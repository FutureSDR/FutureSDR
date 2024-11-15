//! ## SDR Runtime

use futures::channel::mpsc;
use futures::channel::oneshot;
use futuresdr_types::PmtConversionError;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
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
#[cfg(not(target_arch = "wasm32"))]
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
pub use message_io::MessageInput;
pub use message_io::MessageIo;
pub use message_io::MessageIoBuilder;
pub use message_io::MessageOutput;
#[cfg(not(target_arch = "wasm32"))]
pub use mocker::Mocker;
pub use runtime::Runtime;
pub use runtime::RuntimeHandle;
pub use stream_io::StreamInput;
pub use stream_io::StreamIo;
pub use stream_io::StreamIoBuilder;
pub use stream_io::StreamOutput;
pub use tag::copy_tag_propagation;
pub use tag::ItemTag;
pub use tag::Tag;
pub use topology::Topology;

pub use futuresdr_types::BlockDescription;
pub use futuresdr_types::FlowgraphDescription;
pub use futuresdr_types::Pmt;
pub use futuresdr_types::PortId;

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
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Block does not exist
    #[error("Block {0} does not exist")]
    InvalidBlock(usize),
    /// Flowgraph does not exist or terminated
    #[error("Flowgraph terminated")]
    FlowgraphTerminated,
    /// Message port does not exist
    #[error("Block '{0}' does not have message port '{1}'")]
    InvalidMessagePort(BlockPortCtx, PortId),
    /// Stream port does not exist
    #[error("Block '{0}' does not have stream port '{1}'")]
    InvalidStreamPort(BlockPortCtx, PortId),
    /// Invalid Parameter
    #[error("Invalid Parameter")]
    InvalidParameter,
    /// Connect Error
    #[error("Connect error: {0}")]
    ConnectError(Box<ConnectCtx>),
    /// Error in handler
    #[error("Error in handler")]
    HandlerError,
    /// Block is already terminated
    #[error("Block already terminated")]
    BlockTerminated,
    /// Runtime error
    #[error("Runtime error ({0})")]
    RuntimeError(String),
    /// Validation error
    #[error("Validation error {0}")]
    ValidationError(String),
    /// PMT Conversion Error
    #[error("PMT conversion error")]
    PmtConversionError,
    /// Seify Args Conversion Error
    #[error("Seify Args conversion error")]
    SeifyArgsConversionError,
    /// Seify Error
    #[error("Seify error ({0})")]
    SeifyError(String),
    /// Duplicate block name
    #[error("A Block with an instance name of '{0}' already exists")]
    DuplicateBlockName(String),
}

#[cfg(feature = "seify")]
impl From<seify::Error> for Error {
    fn from(value: seify::Error) -> Self {
        Error::SeifyError(value.to_string())
    }
}

impl From<PmtConversionError> for Error {
    fn from(_value: PmtConversionError) -> Self {
        Error::PmtConversionError
    }
}

/// Container for information supporting `ConnectError`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectCtx {
    /// Source block ID
    pub src_block_id: usize,
    /// Source block name
    pub src_block_name: String,
    /// Source block output port
    pub src_port: String,
    /// Source port item type
    pub src_type: String,
    /// Destination block ID
    pub dst_block_id: usize,
    /// Destination block name
    pub dst_block_name: String,
    /// Destination input port
    pub dst_port: String,
    /// Destination port item type
    pub dst_type: String,
}

impl ConnectCtx {
    #[allow(clippy::too_many_arguments)]
    fn new(
        src_block_id: usize,
        src: &Block,
        src_port: &PortId,
        src_output: &StreamOutput,
        dst_block_id: usize,
        dst: &Block,
        dst_port: &PortId,
        dst_input: &StreamInput,
    ) -> Self {
        Self {
            src_block_id,
            src_block_name: src.instance_name().unwrap_or(src.type_name()).to_string(),
            src_port: src_port.to_string(),
            src_type: src_output.type_name().to_string(),
            dst_block_id,
            dst_block_name: dst.instance_name().unwrap_or(src.type_name()).to_string(),
            dst_port: dst_port.to_string(),
            dst_type: dst_input.type_name().to_string(),
        }
    }
}

impl Display for ConnectCtx {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "incompatible ports: {}.{}<{}> -> {}.{}<{}>",
            self.src_block_name,
            self.src_port,
            self.src_type,
            self.dst_block_name,
            self.dst_port,
            self.dst_type
        )
    }
}

/// Description of the [`Block`] under which an [`InvalidMessagePort`] or
/// [`InvalidStreamPort`] error occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockPortCtx {
    /// BlockId is not specified
    None,
    /// Block is identified by its ID in the [`Flowgraph`]
    Id(usize),
    /// Block is identified by its `type_name`
    Name(String),
}

impl From<&Block> for BlockPortCtx {
    fn from(value: &Block) -> Self {
        BlockPortCtx::Name(value.type_name().into())
    }
}

impl Display for BlockPortCtx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BlockPortCtx::None => write!(f, "<None>"),
            BlockPortCtx::Id(id) => write!(f, "ID {id}"),
            BlockPortCtx::Name(name) => write!(f, "{name}"),
        }
    }
}
