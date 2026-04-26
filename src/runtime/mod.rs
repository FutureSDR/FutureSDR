//! Build, run, and control SDR flowgraphs.
//!
//! This module contains the user-facing runtime APIs for:
//! - constructing [`Flowgraph`](crate::runtime::Flowgraph)s
//! - starting them on a [`Runtime`](crate::runtime::Runtime)
//! - interacting with running graphs through
//!   [`RunningFlowgraph`](crate::runtime::RunningFlowgraph) and handles
//! - inspecting finished graphs
//!
//! For custom blocks and runtime extensions, see
//! [`dev`](crate::runtime::dev).
use futuresdr::channel::mpsc;
use futuresdr::channel::oneshot;
use futuresdr_types::PmtConversionError;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use thiserror::Error;

mod block;
mod block_inbox;
mod block_meta;
/// Advanced buffer APIs for implementing custom runtime integrations.
pub mod buffer;
pub mod config;
/// Developer-facing APIs for implementing custom blocks and runtime extensions.
pub mod dev;

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
mod flowgraph_task;
mod kernel;
mod kernel_interface;
mod message_output;
#[cfg(not(target_arch = "wasm32"))]
/// Mocker for unit testing and benchmarking
pub mod mocker;
mod running_flowgraph;
#[allow(clippy::module_inception)]
mod runtime;
/// Advanced scheduler APIs for implementing custom executors.
pub mod scheduler;
mod tag;
mod work_io;

pub use flowgraph::BlockRef;
pub use flowgraph::Flowgraph;
pub use flowgraph_handle::FlowgraphBlockHandle;
pub use flowgraph_handle::FlowgraphHandle;
pub use flowgraph_task::FlowgraphTask;
pub use running_flowgraph::RunningFlowgraph;
pub use runtime::Runtime;
pub use runtime::RuntimeHandle;

pub use futuresdr_types::BlockDescription;
pub use futuresdr_types::BlockId;
pub use futuresdr_types::BlockPortId;
pub use futuresdr_types::FlowgraphDescription;
pub use futuresdr_types::FlowgraphId;
pub use futuresdr_types::Pmt;
pub use futuresdr_types::PmtKind;
pub use futuresdr_types::PortId;

/// Proc-macro and runtime plumbing that is public only so downstream macro
/// expansions can reference generated implementation details.
#[doc(hidden)]
pub mod __private {
    pub use super::flowgraph::ConnectAdd;
    pub use super::kernel_interface::KernelInterface;
}

/// Generic Result Type used for the [`crate::runtime::dev::Kernel`] trait.
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
#[doc(hidden)]
#[derive(Debug)]
pub enum FlowgraphMessage {
    /// Terminate
    Terminate,
    /// Initialize
    Initialized,
    /// Block is Done
    BlockDone {
        /// The Block that is done.
        block_id: BlockId,
    },
    /// Block Error
    BlockError {
        /// The Block that ran into an error.
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
#[doc(hidden)]
#[derive(Debug)]
pub enum BlockMessage {
    /// Initialize
    Initialize,
    /// Terminate
    Terminate,
    /// Get [`BlockDescription`]
    BlockDescription {
        /// Channel for return value
        tx: oneshot::Sender<BlockDescription>,
    },
    /// Stream input port is done
    StreamInputDone {
        /// Stream input Id
        input_id: PortId,
    },
    /// Stream output port is done
    StreamOutputDone {
        /// Stream output Id
        output_id: PortId,
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
    #[error("Block {:?} does not exist", 0)]
    InvalidBlock(BlockId),
    /// Flowgraph does not exist or terminated
    #[error("Flowgraph terminated")]
    FlowgraphTerminated,
    /// Message port does not exist
    #[error("Block '{0}' does not have message port '{1:?}'")]
    InvalidMessagePort(BlockPortCtx, PortId),
    /// Stream port does not exist
    #[error("Block '{0}' does not have stream port '{1:?}'")]
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
    /// Duplicate block name
    #[error("A Block with an instance name of '{0}' already exists")]
    DuplicateBlockName(String),
    /// Error while locking a Mutex that should not be contended or poisoned
    #[error("Error while locking a Mutex that should not be contended or poisoned")]
    LockError,
    /// Seify Args Conversion Error
    #[cfg(feature = "seify")]
    #[error("Seify Args conversion error")]
    SeifyArgsConversionError,
    /// Seify Error
    #[cfg(feature = "seify")]
    #[error("Seify error ({0})")]
    SeifyError(String),
}

#[cfg(feature = "seify")]
impl From<seify::Error> for Error {
    fn from(value: seify::Error) -> Self {
        Error::SeifyError(value.to_string())
    }
}

impl From<oneshot::Canceled> for Error {
    fn from(_value: oneshot::Canceled) -> Self {
        Error::RuntimeError(
            "Couldn't receive from oneshot channel, sender dropped unexpectedly".to_string(),
        )
    }
}

impl From<mpsc::SendError> for Error {
    fn from(_value: mpsc::SendError) -> Self {
        Error::RuntimeError(
            "Couldn't send to mpsc channel, receiver dropped unexpectedly".to_string(),
        )
    }
}

impl<T> From<mpsc::TrySendError<T>> for Error {
    fn from(_value: mpsc::TrySendError<T>) -> Self {
        let message = match _value {
            mpsc::TrySendError::Full(_) => "Couldn't send to mpsc channel, channel is full",
            mpsc::TrySendError::Disconnected(_) => {
                "Couldn't send to mpsc channel, receiver dropped unexpectedly"
            }
        };
        Error::RuntimeError(message.to_string())
    }
}

impl From<PmtConversionError> for Error {
    fn from(_value: PmtConversionError) -> Self {
        Error::PmtConversionError
    }
}

/// Description of the [`Block`] under which an [`Error::InvalidMessagePort`] or
/// [`Error::InvalidStreamPort`] error occurred.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockPortCtx {
    /// BlockId is not specified
    None,
    /// Block is identified by its ID in the [`Flowgraph`]
    Id(BlockId),
    /// Block is identified by its `type_name`
    Name(String),
}

impl From<&dyn crate::runtime::dev::Block> for BlockPortCtx {
    fn from(value: &dyn crate::runtime::dev::Block) -> Self {
        BlockPortCtx::Name(value.type_name().into())
    }
}

impl Display for BlockPortCtx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BlockPortCtx::None => write!(f, "<None>"),
            BlockPortCtx::Id(id) => write!(f, "{id:?}"),
            BlockPortCtx::Name(name) => write!(f, "{name}"),
        }
    }
}
