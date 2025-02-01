//! ## SDR Runtime

use futures::channel::mpsc;
use futures::channel::oneshot;
use futuresdr_types::PmtConversionError;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
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
mod flowgraph_handle;
mod kernel;
pub mod message_io;
#[cfg(not(target_arch = "wasm32"))]
mod mocker;
#[allow(clippy::module_inception)]
mod runtime;
pub mod scheduler;
mod tag;
mod work_io;

pub use block::Block;
pub use block::WrappedKernel;
pub use block_meta::BlockMeta;
pub use flowgraph::Flowgraph;
pub use flowgraph_handle::FlowgraphHandle;
pub use kernel::Kernel;
pub use kernel::KernelInterface;
pub use message_io::MessageOutput;
pub use message_io::MessageOutputs;
#[cfg(not(target_arch = "wasm32"))]
pub use mocker::Mocker;
pub use runtime::Runtime;
pub use runtime::RuntimeHandle;
pub use tag::ItemTag;
pub use tag::Tag;
pub use work_io::WorkIo;

pub use futuresdr_types::BlockDescription;
pub use futuresdr_types::BlockId;
pub use futuresdr_types::FlowgraphDescription;
pub use futuresdr_types::Pmt;
pub use futuresdr_types::PmtKind;
pub use futuresdr_types::PortId;

use buffer::BufferReader;
use buffer::BufferWriter;

/// Generic Result Type used for the [`Kernel`] trait.
///
/// At the moment, a type alias for [`anyhow::Result`].
pub type Result<T, E = anyhow::Error> = anyhow::Result<T, E>;

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
    /// Block is Done
    BlockDone {
        block_id: BlockId,
    },
    /// Block Error
    BlockError {
        block_id: BlockId,
    },
    /// Call handler of block (ignoring result)
    BlockCall {
        /// Block Id
        block_id: BlockId,
        /// Message handler Id
        port_id: PortId,
        /// Input data
        data: Pmt,
        /// Back channel for result
        tx: oneshot::Sender<Result<(), Error>>,
    },
    /// Call handler of block
    BlockCallback {
        /// Block Id
        block_id: BlockId,
        /// Message handler Id
        port_id: PortId,
        /// Input data
        data: Pmt,
        /// Back channel for result
        tx: oneshot::Sender<Result<Pmt, Error>>,
    },
    /// Get [`FlowgraphDescription`]
    FlowgraphDescription {
        /// Back channel for result
        tx: oneshot::Sender<FlowgraphDescription>,
    },
    /// Get [`BlockDescription`]
    BlockDescription {
        /// Block Id
        block_id: BlockId,
        /// Back channel for result
        tx: oneshot::Sender<Result<BlockDescription, Error>>,
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
        tx: oneshot::Sender<Result<Pmt, Error>>,
    },
}

/// FutureSDR Error
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Block does not exist
    #[error("Block {0} does not exist")]
    InvalidBlock(BlockId),
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
    /// Error in handler
    #[error("Error in message handler: {0}")]
    HandlerError(String),
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
    /// Error returned from a Receiver when the corresponding Sender is dropped
    #[error(transparent)]
    ChannelCanceled(#[from] oneshot::Canceled),
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

impl From<&dyn Block> for BlockPortCtx {
    fn from(value: &dyn Block) -> Self {
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
